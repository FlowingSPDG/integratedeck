//! VSD2 WebSocket client: plaintext Hello, X25519 key agreement, encrypted frames.

use crypto_box::PublicKey;
use futures::{SinkExt, StreamExt};
use prost::Message;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::messages::{
    client_message, server_message, Authenticate, ClientInformation, ClientMessage,
    HelloFromClient, ServerMessage,
};
use crate::VsdCrypto;

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct VsdClient {
    sink: futures::stream::SplitSink<Ws, WsMessage>,
    stream: futures::stream::SplitStream<Ws>,
    crypto: VsdCrypto,
}

impl VsdClient {
    /// Connect, exchange Hellos and send the encrypted Authenticate message.
    pub async fn connect(
        url: &str,
        client_information: ClientInformation,
    ) -> anyhow::Result<(Self, crate::HelloFromServer)> {
        let (socket, _) = connect_async(url).await?;
        let (mut sink, mut stream) = socket.split();

        let (secret, public) = VsdCrypto::generate_keypair();
        let hello = ClientMessage {
            msg: Some(client_message::Msg::Hello(HelloFromClient {
                public_key: public.as_bytes().to_vec(),
                client_information: Some(client_information),
            })),
        };
        sink.send(WsMessage::Binary(hello.encode_to_vec().into()))
            .await?;

        let plaintext = next_binary(&mut stream).await?;
        let server = ServerMessage::decode(plaintext.as_slice())?;
        let hello = match server.msg {
            Some(server_message::Msg::Hello(hello)) => hello,
            _ => anyhow::bail!("VSD2 server did not send HelloFromServer"),
        };
        let peer = PublicKey::from_slice(&hello.public_key)
            .map_err(|_| anyhow::anyhow!("invalid VSD2 server public key"))?;
        let crypto = VsdCrypto::new(&secret, &peer);

        let mut client = Self {
            sink,
            stream,
            crypto,
        };
        client
            .send(&ClientMessage {
                msg: Some(client_message::Msg::Authenticate(Authenticate {
                    request_id: "integratedeck".into(),
                })),
            })
            .await?;

        Ok((client, hello))
    }

    /// Encrypt and send one protobuf message.
    pub async fn send(&mut self, message: &ClientMessage) -> anyhow::Result<()> {
        let frame = self.crypto.encrypt(&message.encode_to_vec())?;
        self.sink.send(WsMessage::Binary(frame.into())).await?;
        Ok(())
    }

    /// Read and decrypt the next raw protobuf payload.
    pub async fn recv_raw(&mut self) -> anyhow::Result<Option<Vec<u8>>> {
        loop {
            let Some(message) = self.stream.next().await else {
                return Ok(None);
            };
            match message? {
                WsMessage::Binary(bytes) => {
                    let plaintext = self
                        .crypto
                        .decrypt(bytes.as_ref())
                        .map_err(|e| anyhow::anyhow!("VSD2 decrypt: {e}"))?;
                    return Ok(Some(plaintext));
                }
                WsMessage::Close(_) => return Ok(None),
                _ => {}
            }
        }
    }

    /// Read and decrypt the next protobuf message.
    pub async fn recv(&mut self) -> anyhow::Result<Option<ServerMessage>> {
        match self.recv_raw().await? {
            Some(plaintext) => Ok(Some(ServerMessage::decode(plaintext.as_slice())?)),
            None => Ok(None),
        }
    }
}

async fn next_binary(stream: &mut futures::stream::SplitStream<Ws>) -> anyhow::Result<Vec<u8>> {
    loop {
        let Some(message) = stream.next().await else {
            anyhow::bail!("VSD2 server closed before Hello");
        };
        match message? {
            WsMessage::Binary(bytes) => return Ok(bytes.to_vec()),
            WsMessage::Close(_) => anyhow::bail!("VSD2 server closed before Hello"),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{HelloFromServer, ServiceEndpoints};
    use crypto_box::SecretKey;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

    fn info() -> ClientInformation {
        ClientInformation {
            name: "integratedeck".into(),
            user_agent: "ideck-test".into(),
            device_type: 2,
            device_model: "software".into(),
            os_name: "windows".into(),
            os_version: "11".into(),
        }
    }

    #[tokio::test]
    async fn handshake_and_authenticate_with_mock_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();

            let hello = match ws.next().await.unwrap().unwrap() {
                WsMessage::Binary(bytes) => ClientMessage::decode(bytes.as_ref()).unwrap(),
                other => panic!("unexpected {other:?}"),
            };
            let client_public = match hello.msg.unwrap() {
                client_message::Msg::Hello(hello) => hello.public_key,
                _ => panic!("expected hello"),
            };

            let server_secret = SecretKey::generate(&mut rand_core::OsRng);
            let server_hello = ServerMessage {
                msg: Some(server_message::Msg::Hello(HelloFromServer {
                    workstation_id: "ws-test".into(),
                    hostname: "host".into(),
                    version: "7.6.0".into(),
                    os_type: 3,
                    public_key: server_secret.public_key().as_bytes().to_vec(),
                    client_needs_authentication: true,
                    os_name: "windows".into(),
                    os_version: "11".into(),
                    service_endpoints: Some(ServiceEndpoints {
                        addresses: vec!["127.0.0.1".into()],
                        port: addr.port() as u32,
                    }),
                })),
            };
            ws.send(WsMessage::Binary(server_hello.encode_to_vec().into()))
                .await
                .unwrap();

            let peer = PublicKey::from_slice(&client_public).unwrap();
            let crypto = VsdCrypto::new(&server_secret, &peer);
            let frame = match ws.next().await.unwrap().unwrap() {
                WsMessage::Binary(bytes) => bytes,
                other => panic!("unexpected {other:?}"),
            };
            let plaintext = crypto.decrypt(frame.as_ref()).unwrap();
            let auth = ClientMessage::decode(plaintext.as_slice()).unwrap();
            assert!(matches!(
                auth.msg,
                Some(client_message::Msg::Authenticate(_))
            ));
        });

        let (_client, hello) = VsdClient::connect(&format!("ws://{addr}"), info())
            .await
            .expect("handshake");
        assert_eq!(hello.workstation_id, "ws-test");
        server.await.unwrap();
    }
}
