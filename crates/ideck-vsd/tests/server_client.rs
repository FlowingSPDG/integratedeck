//! End-to-end VSD2 handshake against the integratedeck-owned server.

use ideck_vsd::client::VsdClient;
use ideck_vsd::messages::{
    client_message, server_message, ClientInformation, ClientMessage, CreateVirtualDevice,
    OpenVirtualDevice,
};
use ideck_vsd::server::{VsdImage, VsdServer, VsdServerConfig};

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

#[tokio::test]
async fn server_pushes_button_images_to_the_opened_device() {
    let server = VsdServer::bind("127.0.0.1:0", VsdServerConfig::default())
        .await
        .expect("bind");
    let addr = server.local_addr().expect("addr");
    let images = server.image_sender();
    tokio::spawn(server.run());

    let (mut client, _hello) = VsdClient::connect(
        &format!("ws://{addr}"),
        ClientInformation {
            name: "ideck-image-test".into(),
            user_agent: "ideck-test/0.1".into(),
            device_type: 2,
            device_model: "software".into(),
            os_name: "windows".into(),
            os_version: "11".into(),
        },
    )
    .await
    .expect("connect");

    let _keypad = client.recv().await.expect("keypad").expect("some");
    let _auth_ok = client.recv().await.expect("auth").expect("some");

    client
        .send(&ClientMessage {
            msg: Some(client_message::Msg::OpenVirtualDevice(OpenVirtualDevice {
                device_id: "integratedeck-primary".into(),
            })),
        })
        .await
        .expect("open");
    let opened = client.recv().await.expect("open ok").expect("some");
    assert!(matches!(opened.msg, Some(server_message::Msg::Ok(_))));

    images
        .send(VsdImage {
            device_id: "integratedeck-primary".into(),
            index: 2,
            jpeg: vec![0xff, 0xd8, 0xff, 0xd9],
        })
        .expect("send image");

    let image = client.recv().await.expect("image").expect("some");
    match image.msg {
        Some(server_message::Msg::ButtonImage(image)) => {
            assert_eq!(image.index, 2);
            assert_eq!(image.jpeg, vec![0xff, 0xd8, 0xff, 0xd9]);
        }
        other => panic!("unexpected message: {other:?}"),
    }
}
