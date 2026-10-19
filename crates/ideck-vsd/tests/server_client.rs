//! End-to-end VSD2 handshake against the integratedeck-owned server.

use ideck_vsd::client::VsdClient;
use ideck_vsd::messages::{
    client_message, server_message, ClientInformation, ClientMessage, CreateVirtualDevice,
};
use ideck_vsd::server::{VsdServer, VsdServerConfig};

#[tokio::test]
async fn server_accepts_client_and_creates_virtual_device() {
    let server = VsdServer::bind("127.0.0.1:0", VsdServerConfig::default())
        .await
        .expect("bind");
    let addr = server.local_addr().expect("addr");
    tokio::spawn(server.run());

    let (mut client, hello) = VsdClient::connect(
        &format!("ws://{addr}"),
        ClientInformation {
            name: "integratedeck test".into(),
            user_agent: "ideck-test/0.1".into(),
            device_type: 2,
            device_model: "software".into(),
            os_name: "windows".into(),
            os_version: "11".into(),
        },
    )
    .await
    .expect("connect");
    assert!(!hello.client_needs_authentication);

    let keypad = client.recv().await.expect("recv keypad").expect("some");
    match keypad.msg {
        Some(server_message::Msg::KeypadConfig(config)) => {
            assert_eq!(config.rows, 3);
            assert_eq!(config.columns, 5);
        }
        other => panic!("unexpected message: {other:?}"),
    }

    let ok = client.recv().await.expect("recv ok").expect("some");
    assert!(matches!(ok.msg, Some(server_message::Msg::Ok(_))));

    client
        .send(&ClientMessage {
            msg: Some(client_message::Msg::CreateVirtualDevice(CreateVirtualDevice {
                name: "Software Deck".into(),
            })),
        })
        .await
        .expect("create");

    let created = client.recv().await.expect("recv ok").expect("some");
    assert!(matches!(created.msg, Some(server_message::Msg::Ok(_))));

    client
        .send(&ClientMessage {
            msg: Some(client_message::Msg::KeyEvent(ideck_vsd::messages::KeyEvent {
                index: 0,
                pressed: true,
            })),
        })
        .await
        .expect("key event");

    let list = client.recv().await.expect("recv list").expect("some");
    match list.msg {
        Some(server_message::Msg::VirtualDevices(list)) => {
            assert!(list.devices.len() >= 2, "expected initial + created device");
            assert!(list
                .devices
                .iter()
                .any(|device| device.name == "Software Deck"));
        }
        other => panic!("unexpected message: {other:?}"),
    }
}
