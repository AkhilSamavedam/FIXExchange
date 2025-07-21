use std::io::BufRead;

use core_affinity;
use tokio::sync::mpsc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
#[cfg(target_os = "linux")]
use fork_union::{ThreadPool, Prong};

mod exchange;
mod fix;
mod engine;
mod types;

use exchange::Exchange;
use fix::handle_fix_message;
use engine::EngineMessage;

enum RawInput {
    Payload(String),
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

    #[cfg(target_os = "linux")]
    let mut consumer_pool = ThreadPool::try_named_spawn("consumer", 1).expect("Failed to start consumer pool");
    #[cfg(target_os = "linux")]
    let mut producer_pool = ThreadPool::try_named_spawn("producer", 2).expect("Failed to start producer pool");
    #[cfg(target_os = "linux")]
    let mut outbound_pool = ThreadPool::try_named_spawn("outbound", 1).expect("Failed to start outbound pool");

    let (tx, mut rx): (UnboundedSender<EngineMessage>, UnboundedReceiver<EngineMessage>) = mpsc::unbounded_channel();
    let (outbound_tx, mut outbound_rx): (UnboundedSender<EngineMessage>, UnboundedReceiver<EngineMessage>) = mpsc::unbounded_channel();

    let (raw_tx, mut raw_rx) = mpsc::unbounded_channel::<RawInput>();

    let listener = tokio::net::TcpListener::bind("0.0.0.0:9000").await?;
    println!("Exchange server TCP socket on 0.0.0.0:9000");

    let raw_tx_clone = raw_tx.clone();
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let tx_inner = raw_tx_clone.clone();
                    tokio::spawn(async move {
                        let reader = tokio::io::BufReader::new(stream);
                        let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
                        while let Ok(Some(line)) = lines.next_line().await {
                            if tx_inner.send(RawInput::Payload(line.trim().to_string())).is_err() {
                                eprintln!("Parser thread unavailable.");
                                break;
                            }
                        }
                    });
                }
                Err(e) => {
                    eprintln!("TCP connection failed: {}", e);
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            }
        }
    });

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
    producer_pool.for_threads(move |_thread_index, _colocation_index| {
        while let Ok(raw_input) = raw_rx.blocking_recv() {
            if let RawInput::Payload(raw_msg) = raw_input {
                let engine_message = handle_fix_message(&raw_msg);
                match &engine_message {
                    EngineMessage::InvalidMessage { reason, .. } => {
                        eprintln!("Invalid FIX message: {}", reason);
                    }
                    _ => {
                        if tx.send(engine_message).is_err() {
                            eprintln!("Failed to forward parsed message to exchange.");
                        }
                    }
                }
            }
        }
    });

    #[cfg(not(target_os = "linux"))]
    {
        let tx = tx.clone();
        let parser_core = parser_core;
        tokio::spawn(async move {
            if let Some(core) = parser_core {
                core_affinity::set_for_current(core);
                println!("Pinned parsing thread to core {:?}", core.id);
            }

            while let Some(raw_input) = raw_rx.recv().await {
                let RawInput::Payload(raw_msg) = raw_input;
                let engine_message = handle_fix_message(&raw_msg);
                match &engine_message {
                    EngineMessage::InvalidMessage { reason, .. } => {
                        eprintln!("Invalid FIX message: {}", reason);
                    }
                    _ => {
                        if tx.send(engine_message).is_err() {
                            eprintln!("Failed to forward parsed message to exchange.");
                        }
                    }
                }
            }
        });
    }

    #[cfg(target_os = "linux")]
    outbound_pool.for_threads(move |_thread_index, _colocation_index| {
        while let Ok(message) = outbound_rx.blocking_recv() {
            // TODO: Convert EngineMessage to FIX string and send via TCP
            println!("Outbound: {:?}", message);
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

    Ok(())
}