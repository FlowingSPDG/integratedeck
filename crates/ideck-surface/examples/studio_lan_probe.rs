//! Probe the Stream Deck Studio legacy LAN protocol.
//!
//! ```text
//! cargo run -p ideck-surface --example studio_lan_probe -- 192.168.3.48:5343
//! ```

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "192.168.3.48:5343".to_string());
    let mut stream = TcpStream::connect(&addr).await?;
    stream.set_nodelay(true).ok();
    println!("connected {addr}");

    let mut keepalive = [0u8; 512];
    stream.read_exact(&mut keepalive).await?;
    println!("keepalive head={}", hex(&keepalive[..16]));

    let mut ack = [0u8; 1024];
    ack[0] = 0x03;
    ack[1] = 0x1a;
    stream.write_all(&ack).await?;

    let mut brightness = [0u8; 1024];
    brightness[0] = 0x03;
    brightness[1] = 0x08;
    brightness[2] = 64;
    stream.write_all(&brightness).await?;

    for (index, id) in [0x1cu8, 0x80].into_iter().enumerate() {
        let mut query = [0u8; 1024];
        query[0] = 0x03;
        query[1] = id;
        stream.write_all(&query).await?;

        let mut packet = [0u8; 512];
        match tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut packet)).await {
            Ok(Ok(_)) => println!("query {id:#04x} response {index} head={}", hex(&packet[..32])),
            Ok(Err(error)) => {
                println!("query {id:#04x} read error: {error}");
                break;
            }
            Err(_) => {
                println!("query {id:#04x} timeout");
                break;
            }
        }
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
