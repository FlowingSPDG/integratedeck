//! Connect to a VSD2 server, authenticate, and dump decrypted protobuf frames.
//!
//! ```text
//! cargo run -p ideck-vsd --example vsd_dump -- 127.0.0.1:28198
//! ```

use std::time::Duration;

use ideck_vsd::client::VsdClient;
use ideck_vsd::ClientInformation;
use prost::Message;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:28198".to_string());
    let info = ClientInformation {
        name: "integratedeck-vsd-dump".into(),
        user_agent: "ideck/0.1".into(),
        device_type: 2,
        device_model: "software".into(),
        os_name: "windows".into(),
        os_version: "11".into(),
    };

    let (mut client, hello) = VsdClient::connect(&format!("ws://{addr}"), info).await?;
    println!(
        "hello workstation={} host={} needs_auth={} endpoints={:?}",
        hello.workstation_id,
        hello.hostname,
        hello.client_needs_authentication,
        hello.service_endpoints
    );

    let mut created = false;
    let mut pressed = false;
    for index in 0..12 {
        match tokio::time::timeout(Duration::from_secs(30), client.recv_raw()).await {
            Ok(Ok(Some(bytes))) => {
                println!("frame {index} len={} hex={}", bytes.len(), hex(&bytes));
                match ideck_vsd::ServerMessage::decode(bytes.as_slice()) {
                    Ok(message) => {
                        println!("  decoded={message:?}");
                        if !pressed
                            && matches!(
                                message.msg,
                                Some(ideck_vsd::server_message::Msg::ButtonImage(_))
                            )
                        {
                            pressed = true;
                            tokio::time::sleep(Duration::from_secs(5)).await;
                            let event = ideck_vsd::ClientMessage {
                                msg: Some(ideck_vsd::client_message::Msg::KeyEvent(
                                    ideck_vsd::messages::KeyEvent {
                                        index: 0,
                                        pressed: true,
                                    },
                                )),
                            };
                            if let Err(error) = client.send(&event).await {
                                eprintln!("  key event failed: {error}");
                            }
                        }
                        if !created
                            && matches!(
                                message.msg,
                                Some(ideck_vsd::server_message::Msg::KeypadConfig(_))
                            )
                        {
                            created = true;
                            let create = ideck_vsd::ClientMessage {
                                msg: Some(ideck_vsd::client_message::Msg::CreateVirtualDevice(
                                    ideck_vsd::CreateVirtualDevice {
                                        name: "integratedeck".into(),
                                    },
                                )),
                            };
                            if let Err(error) = client.send(&create).await {
                                eprintln!("  create failed: {error}");
                            }
                        }
                    }
                    Err(error) => println!("  decode error: {error}"),
                }
            }
            Ok(Ok(None)) => {
                println!("closed");
                break;
            }
            Ok(Err(error)) => {
                eprintln!("frame error: {error}");
                break;
            }
            Err(_) => {
                println!("timeout");
                break;
            }
        }
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
