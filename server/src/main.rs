use core_affinity;
use tokio::sync::mpsc;
use std::os::unix::net::UnixListener;
use std::io::BufRead;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

mod exchange;
mod fix;
mod engine;
mod types;

use exchange::Exchange;
use fix::handle_fix_message;
use engine::EngineMessage;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set NUMA-aware thread affinity for main thread
    if let Some(core_ids) = core_affinity::get_core_ids() {
        // For simplicity, just pin the main thread to the first available core
        core_affinity::set_for_current(core_ids[0]);
        println!("Pinned main thread to core {:?}", core_ids[0].id);
    }

    let mut exchange = Exchange::new();

    let (tx, mut rx): (UnboundedSender<EngineMessage>, UnboundedReceiver<EngineMessage>) = mpsc::unbounded_channel();

    let (raw_tx, mut raw_rx) = mpsc::unbounded_channel::<String>();

    // Set up IPC listener
    let socket_path = "/tmp/fix_exchange.sock";
    let _ = std::fs::remove_file(socket_path); // Ensure clean state
    let listener = UnixListener::bind(socket_path)?;
    println!("Exchange server IPC socket on {}", socket_path);

    tokio::spawn(async move {
        while let Some(engine_message) = rx.recv().await {
            exchange.handle_message(engine_message);
        }
    });

    {
        let tx = tx.clone();
        tokio::spawn(async move {
            if let Some(core_ids) = core_affinity::get_core_ids() {
                core_affinity::set_for_current(core_ids[1 % core_ids.len()]);
                println!("Pinned parsing thread to core {:?}", core_ids[1 % core_ids.len()]);
            }

            while let Some(raw_msg) = raw_rx.recv().await {
                let engine_message = handle_fix_message(&raw_msg);
                match &engine_message {
                    EngineMessage::InvalidMessage { reason, .. } => {
                        eprintln!("Invalid FIX message: {}", reason);
                        // Optionally log or send back to sender
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

    // Spawn blocking IPC listener in a separate task
    let ipc_handle = tokio::task::spawn_blocking(move || {
        for stream in listener.incoming() {
            let raw_tx = raw_tx.clone();
            match stream {
                Ok(stream) => {
                    std::thread::spawn(move || {
                        let reader = std::io::BufReader::new(stream);
                        for line in reader.lines() {
                            if let Ok(line) = line {
                                println!("Received: {}", line.trim());
                                if raw_tx.send(line.trim().to_string()).is_err() {
                                    eprintln!("Parser thread unavailable.");
                                    break;
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    eprintln!("IPC connection failed: {}", e);
                }
            }
        }
    });
    // Optionally, wait for the blocking IPC listener to finish or propagate errors if necessary.
    // For a server, you might not want to await this unless you have a shutdown mechanism.
    // Here we just await to handle any panics/errors before exiting main.
    let _ = ipc_handle.await?;
    Ok(())
}