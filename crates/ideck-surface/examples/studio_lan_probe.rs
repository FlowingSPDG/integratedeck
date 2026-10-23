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

    let mut color = [0u8; 1024];
    color[0] = 0x03;
    color[1] = 0x06;
    color[2] = 0x00;
    color[3] = 0xff;
    color[4] = 0x00;
    color[5] = 0x00;
    stream.write_all(&color).await?;
    let mut color_response = [0u8; 512];
    match tokio::time::timeout(
        Duration::from_secs(2),
        stream.read_exact(&mut color_response),
    )
    .await
    {
        Ok(Ok(_)) => println!("color response head={}", hex(&color_response[..16])),
        Ok(Err(error)) => println!("color error: {error}"),
        Err(_) => println!("color timeout"),
    }

    let image = image::RgbaImage::from_pixel(144, 112, image::Rgba([255, 0, 0, 255]));
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode_image(&image::DynamicImage::ImageRgba8(image))?;
    let mut offset = 0usize;
    let mut sequence = 0u16;
    while offset < jpeg.len() {
        let end = (offset + 1016).min(jpeg.len());
        let chunk_len = (end - offset) as u16;
        let mut packet = vec![0u8; 1024];
        packet[0] = 0x02;
        packet[1] = 0x07;
        packet[2] = 0x00;
        packet[3] = u8::from(end == jpeg.len());
        packet[4..6].copy_from_slice(&chunk_len.to_le_bytes());
        packet[6..8].copy_from_slice(&sequence.to_le_bytes());
        packet[8..8 + (end - offset)].copy_from_slice(&jpeg[offset..end]);
        stream.write_all(&packet).await?;
        sequence = sequence.wrapping_add(1);
        offset = end;
    }
    let mut image_response = [0u8; 512];
    match tokio::time::timeout(
        Duration::from_secs(2),
        stream.read_exact(&mut image_response),
    )
    .await
    {
        Ok(Ok(_)) => println!("image response head={}", hex(&image_response[..16])),
        Ok(Err(error)) => println!("image error: {error}"),
        Err(_) => println!("image timeout"),
    }

    for (index, id) in [0x1cu8, 0x80].into_iter().enumerate() {
        let mut query = [0u8; 1024];
        query[0] = 0x03;
        query[1] = id;
        stream.write_all(&query).await?;

        let mut packet = [0u8; 512];
        match tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut packet)).await {
            Ok(Ok(_)) => println!(
                "query {id:#04x} response {index} head={}",
                hex(&packet[..32])
            ),
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
