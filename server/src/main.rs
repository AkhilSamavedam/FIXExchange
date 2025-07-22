use std::io::BufRead;
use std::sync::OnceLock;

use dashmap::DashMap;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::io::{AsyncWriteExt, AsyncBufReadExt};
use core_affinity;
#[cfg(target_os = "linux")]
use fork_union::{ThreadPool};

mod exchange;
mod fix;
mod engine;
mod types;

use types::ClientID;
use exchange::Exchange;
use fix::handle_fix_message;
use engine::EngineMessage;

// Replace TcpStream storage with Sender<String>
static CLIENT_SENDERS: OnceLock<DashMap<ClientID, UnboundedSender<String>>> = OnceLock::new();

async fn handle_connection(stream: tokio::net::TcpStream, tx: UnboundedSender<EngineMessage>) {
    // Split the stream into reader and writer
    let (reader, mut writer) = stream.into_split();
    let mut lines = tokio::io::BufReader::new(reader).lines();

    // Await the first valid message to get client_id and set up outbound channel
    if let Ok(Some(line)) = lines.next_line().await {
        let engine_message = handle_fix_message(&line.trim());
        match &engine_message {
            EngineMessage::InvalidMessage { reason, .. } => {
                eprintln!("Invalid FIX message: {}", reason);
                return;
            }
            EngineMessage::NewOrder {client_id, ..}
            | EngineMessage::CreateInstrument {client_id, ..}
            | EngineMessage::AdvanceTime {client_id, ..}
            | EngineMessage::CancelOrder {client_id, ..}
            | EngineMessage::Snapshot {client_id, ..} => {
                let client_id = client_id.clone();
                let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
                CLIENT_SENDERS.get().unwrap().insert(client_id.clone(), out_tx);

                // Spawn writer task for outbound messages
                tokio::spawn(async move {
                    while let Some(msg) = out_rx.recv().await {
                        if let Err(e) = writer.write_all(msg.as_bytes()).await {
                            eprintln!("Failed to write to client {}: {}", client_id, e);
                            break;
                        }
                    }
                });

                // Send the first message to exchange
                if tx.send(engine_message).is_err() {
                    eprintln!("Failed to forward parsed message to exchange.");
                    return;
                }

                // Reader loop for inbound FIX messages
                while let Ok(Some(line)) = lines.next_line().await {
                    let engine_message = handle_fix_message(&line.trim());
                    if tx.send(engine_message).is_err() {
                        eprintln!("Failed to send message to exchange");
                        break;
                    }
                }
            }
            _ => {
                // For messages without client_id, just forward
                if tx.send(engine_message).is_err() {
                    eprintln!("Failed to forward parsed message to exchange.");
                    return;
                }
                // Continue reading lines and forwarding
                while let Ok(Some(line)) = lines.next_line().await {
                    let engine_message = handle_fix_message(&line.trim());
                    if tx.send(engine_message).is_err() {
                        eprintln!("Failed to send message to exchange");
                        break;
                    }
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Pin main and parser threads to the first two cores (no NUMA awareness)
    let mut parser_core = None;
    if let Some(core_ids) = core_affinity::get_core_ids() {
        if let Some(main_core) = core_ids.get(0) {
            core_affinity::set_for_current(*main_core);
            println!("Pinned main thread to core {:?}", main_core.id);
        }
        parser_core = core_ids.get(1).copied();
    }

    let mut exchange = Exchange::new();

    CLIENT_SENDERS.set(DashMap::new()).unwrap();

    #[cfg(target_os = "linux")]
    let mut consumer_pool = ThreadPool::try_named_spawn("consumer", 1).expect("Failed to start consumer pool");
    #[cfg(target_os = "linux")]
    let mut producer_pool = ThreadPool::try_named_spawn("producer", 2).expect("Failed to start producer pool");
    #[cfg(target_os = "linux")]
    let mut outbound_pool = ThreadPool::try_named_spawn("outbound", 1).expect("Failed to start outbound pool");

    let (tx, mut rx): (UnboundedSender<EngineMessage>, UnboundedReceiver<EngineMessage>) = mpsc::unbounded_channel();
    let (outbound_tx, mut outbound_rx): (UnboundedSender<EngineMessage>, UnboundedReceiver<EngineMessage>) = mpsc::unbounded_channel();

    #[cfg(not(target_os = "linux"))]
    {
        let listener = tokio::net::TcpListener::bind("0.0.0.0:9000").await?;
        println!("Exchange server TCP socket on 0.0.0.0:9000");

        let tx_clone = tx.clone();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let tx_inner = tx_clone.clone();

                        // Spawn a task per connection
                        tokio::spawn(async move {
                            handle_connection(stream, tx_inner).await;
                        });
                    }
                    Err(e) => {
                        eprintln!("TCP connection failed: {}", e);
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
            }
        });
    }

    #[cfg(target_os = "linux")]
    consumer_pool.for_threads(move |_thread_index, _colocation_index| {
        while let Ok(engine_message) = rx.blocking_recv() {
            if let Some(outbound) = exchange.handle_message(engine_message) {
                let _ = outbound_tx.send(outbound);
            }
        }
    });

    #[cfg(not(target_os = "linux"))]
    {
        let outbound_tx = outbound_tx.clone();
        #[cfg(not(target_os = "linux"))]
        tokio::spawn(async move {
            while let Some(engine_message) = rx.recv().await {
                if let Some(outbound) = exchange.handle_message(engine_message) {
                    let _ = outbound_tx.send(outbound);
                }
            }
        });
    }

    #[cfg(target_os = "linux")]
    {
        let tx = tx.clone();
        producer_pool.for_n_dynamic(move |_thread_index| {
            let tx = tx.clone();
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async {
                let listener = tokio::net::TcpListener::bind("0.0.0.0:9000").await.expect("Failed to bind TCP listener");
                println!("Exchange server TCP socket on 0.0.0.0:9000");

                loop {
                    match listener.accept().await {
                        Ok((stream, _)) => {
                            let tx_inner = tx.clone();
                            handle_connection(stream, tx_inner).await;
                        }
                        Err(e) => {
                            eprintln!("TCP connection failed: {}", e);
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                    }
                }
            });
        });
    }

    #[cfg(target_os = "linux")]
    outbound_pool.for_threads(move |_thread_index, _colocation_index| {
        while let Ok(message) = outbound_rx.blocking_recv() {
            if let Some(sender) = CLIENT_SENDERS.get() {
                if let Some(client_id) = extract_client_id(&message) {
                    if let Some(tx) = sender.get(&client_id) {
                        let fix_msg = serialize_engine_message(&message);
                        let _ = tx.send(fix_msg);
                    }
                }
            }
        }
    });

    #[cfg(not(target_os = "linux"))]
    {
        tokio::spawn(async move {
            while let Some(message) = outbound_rx.recv().await {
                println!("Outbound: {:?}", message);
            }
        });
    }

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    }
}