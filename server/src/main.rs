use tokio::net::TcpListener;
use tokio::io::{AsyncBufReadExt, BufReader};

mod exchange;
mod fix;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:4000").await?;
    println!("Exchange server running on 127.0.0.1:4000");

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let mut reader = BufReader::new(stream);
            let mut buffer = String::new();
            while reader.read_line(&mut buffer).await.unwrap() > 0 {
                println!("Received: {}", buffer.trim());
                // TODO: parse FIX string -> Order -> exchange.submit_order(order)
                buffer.clear();
            }
        });
    }
}