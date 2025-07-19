use tokio::sync::mpsc;
use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

mod exchange;
mod fix;
mod engine;
mod types;

use exchange::Exchange;
use fix::handle_fix_message;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut exchange = Exchange::new();

    let (tx, mut rx): (UnboundedSender<String>, UnboundedReceiver<String>) = mpsc::unbounded_channel();

    let listener = TcpListener::bind("127.0.0.1:4000").await?;
    println!("Exchange server running on 127.0.0.1:4000");

    tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            handle_fix_message(&mut exchange, &message);
        }
    });

    loop {
        let (stream, _) = listener.accept().await?;
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stream);
            let mut buffer = String::new();
            while reader.read_line(&mut buffer).await.unwrap() > 0 {
                println!("Received: {}", buffer.trim());
                if tx.send(buffer.clone()).is_err() {
                    eprintln!("Matching engine has shut down.");
                    break;
                }
                buffer.clear();
            }
        });
    }
}