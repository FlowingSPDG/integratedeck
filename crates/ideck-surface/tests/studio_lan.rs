//! Studio LAN driver integration against a mock legacy server.

use std::time::Duration;

use ideck_core::{solid_key_png, SurfaceId, VisualState};
use ideck_surface::{
    CellAddress, CellUpdate, StudioLanDriver, Surface, SurfaceDriverPlugin, SurfaceInput,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn decodes_keys_and_encoders_from_mock_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut keepalive = [0u8; 512];
        keepalive[0] = 0x01;
        keepalive[1] = 0x0a;
        stream.write_all(&keepalive).await.unwrap();
        let mut packet = [0u8; 1024];
        stream.read_exact(&mut packet).await.unwrap();
        stream.read_exact(&mut packet).await.unwrap();

        let mut key = [0u8; 512];
        key[0] = 0x01;
        key[2] = 0x20;
        key[4] = 1;
        stream.write_all(&key).await.unwrap();

        let mut encoder = [0u8; 512];
        encoder[0] = 0x01;
        encoder[1] = 0x03;
        encoder[5] = 1;
        stream.write_all(&encoder).await.unwrap();

        encoder[4] = 0x01;
        encoder[5] = 2;
        stream.write_all(&encoder).await.unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
    });

    let driver = StudioLanDriver;
    let surface = driver
        .connect(&addr, SurfaceId(uuid::Uuid::new_v4()), "studio-test")
        .expect("connect");
    let mut inputs = surface.subscribe_inputs();
    let _loop = surface.clone().spawn_input_loop();

    let mut seen = Vec::new();
    for _ in 0..3 {
        let input = tokio::time::timeout(Duration::from_secs(2), inputs.recv())
            .await
            .expect("input timeout")
            .expect("input channel");
        seen.push(input);
    }

    assert!(seen.iter().any(|input| matches!(
        input,
        SurfaceInput::KeyDown {
            address: ideck_surface::CellAddress { row: 0, column: 0 }
        }
    )));
    assert!(seen.iter().any(|input| matches!(
        input,
        SurfaceInput::EncoderPress {
            index: 0,
            pressed: true
        }
    )));
    assert!(seen.iter().any(|input| matches!(
        input,
        SurfaceInput::EncoderRotate { index: 0, ticks: 2, .. }
    )));

    server.await.unwrap();
}

#[tokio::test]
async fn sends_key_image_packet_to_mock_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut keepalive = [0u8; 512];
        keepalive[0] = 0x01;
        keepalive[1] = 0x0a;
        stream.write_all(&keepalive).await.unwrap();
        let mut packet = [0u8; 1024];
        stream.read_exact(&mut packet).await.unwrap();
        stream.read_exact(&mut packet).await.unwrap();
        stream.read_exact(&mut packet).await.unwrap();
        assert_eq!(packet[0..2], [0x02, 0x07]);
        assert_eq!(packet[2], 0);
        assert!(u16::from_le_bytes([packet[4], packet[5]]) > 0);
    });

    let driver = StudioLanDriver;
    let surface = driver
        .connect(&addr, SurfaceId(uuid::Uuid::new_v4()), "studio-image")
        .expect("connect");
    let visual = VisualState {
        image: Some(solid_key_png(144, 112, 255, 0, 0)),
        ..Default::default()
    };
    Surface::render(
        surface.as_ref(),
        &[CellUpdate {
            address: CellAddress { row: 0, column: 0 },
            visual,
        }],
    )
    .await
    .unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn reconnects_after_server_close() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        for attempt in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut keepalive = [0u8; 512];
            keepalive[0] = 0x01;
            keepalive[1] = 0x0a;
            stream.write_all(&keepalive).await.unwrap();
            let mut packet = [0u8; 1024];
            stream.read_exact(&mut packet).await.unwrap();
            stream.read_exact(&mut packet).await.unwrap();
            if attempt == 0 {
                drop(stream);
                continue;
            }
            let mut key = [0u8; 512];
            key[0] = 0x01;
            key[2] = 0x02;
            key[4] = 1;
            stream.write_all(&key).await.unwrap();
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });

    let driver = StudioLanDriver;
    let surface = driver
        .connect(&addr, SurfaceId(uuid::Uuid::new_v4()), "studio-reconnect")
        .expect("connect");
    let mut inputs = surface.subscribe_inputs();
    let _loop = surface.clone().spawn_input_loop();

    let input = tokio::time::timeout(Duration::from_secs(15), inputs.recv())
        .await
        .expect("reconnect timeout")
        .expect("input channel");
    assert!(matches!(
        input,
        SurfaceInput::KeyDown {
            address: ideck_surface::CellAddress { row: 0, column: 0 }
        }
    ));

    let _ = server.await;
}


