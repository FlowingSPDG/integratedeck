//! Stream Deck VSD2 protocol: protobuf messages and libsodium `crypto_box`.
//!
//! Wire format after the plaintext Hello exchange:
//!
//! ```text
//! nonce[24] || Poly1305 tag[16] || XSalsa20 ciphertext
//! ```
//!
//! The key is `crypto_box_beforenm(peer_public, own_secret)` used directly as
//! the secretbox key for both directions. Hello frames are plaintext protobuf.

use crypto_box::aead::AeadInPlace;
use crypto_box::{Nonce, PublicKey, SalsaBox, SecretKey, Tag};
use rand_core::OsRng;

pub mod client;
pub mod messages;
pub mod server;

pub use messages::*;

pub const NONCE_LEN: usize = 24;
pub const TAG_LEN: usize = 16;
pub const FRAME_OVERHEAD: usize = NONCE_LEN + TAG_LEN;

#[derive(Debug, thiserror::Error)]
pub enum VsdError {
    #[error("frame too short: {0} bytes")]
    ShortFrame(usize),
    #[error("crypto error")]
    Crypto,
}

/// A `crypto_box` session for one peer.
pub struct VsdCrypto {
    cipher: SalsaBox,
}

impl VsdCrypto {
    /// Generate a fresh X25519 keypair.
    pub fn generate_keypair() -> (SecretKey, PublicKey) {
        let secret = SecretKey::generate(&mut OsRng);
        let public = secret.public_key();
        (secret, public)
    }

    /// Build the shared key from our secret and the peer's public key.
    pub fn new(own_secret: &SecretKey, peer_public: &PublicKey) -> Self {
        Self {
            cipher: SalsaBox::new(peer_public, own_secret),
        }
    }

    /// Encrypt one protobuf with a fresh nonce; returns the complete wire frame.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, VsdError> {
        let mut nonce = [0u8; NONCE_LEN];
        rand_core::RngCore::fill_bytes(&mut OsRng, &mut nonce);
        let nonce = Nonce::from_slice(&nonce);

        let mut body = plaintext.to_vec();
        let tag = self
            .cipher
            .encrypt_in_place_detached(nonce, b"", &mut body)
            .map_err(|_| VsdError::Crypto)?;

        let mut frame = Vec::with_capacity(FRAME_OVERHEAD + body.len());
        frame.extend_from_slice(nonce);
        frame.extend_from_slice(tag.as_slice());
        frame.extend_from_slice(&body);
        Ok(frame)
    }

    /// Decrypt one wire frame back to protobuf bytes.
    pub fn decrypt(&self, frame: &[u8]) -> Result<Vec<u8>, VsdError> {
        if frame.len() < FRAME_OVERHEAD {
            return Err(VsdError::ShortFrame(frame.len()));
        }
        let nonce = Nonce::from_slice(&frame[..NONCE_LEN]);
        let tag = Tag::from_slice(&frame[NONCE_LEN..FRAME_OVERHEAD]);
        let mut body = frame[FRAME_OVERHEAD..].to_vec();
        self.cipher
            .decrypt_in_place_detached(nonce, b"", &mut body, tag)
            .map_err(|_| VsdError::Crypto)?;
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;

    #[test]
    fn crypto_round_trip_matches_libsodium_layout() {
        let (alice_sk, alice_pk) = VsdCrypto::generate_keypair();
        let (bob_sk, bob_pk) = VsdCrypto::generate_keypair();
        let alice = VsdCrypto::new(&alice_sk, &bob_pk);
        let bob = VsdCrypto::new(&bob_sk, &alice_pk);

        let plaintext = b"protobuf bytes";
        let frame = alice.encrypt(plaintext).unwrap();
        assert_eq!(frame.len(), plaintext.len() + FRAME_OVERHEAD);
        assert_eq!(bob.decrypt(&frame).unwrap(), plaintext);
    }

    #[test]
    fn rejects_truncated_frames() {
        let (sk, pk) = VsdCrypto::generate_keypair();
        let crypto = VsdCrypto::new(&sk, &pk);
        assert!(matches!(
            crypto.decrypt(&[0u8; 39]),
            Err(VsdError::ShortFrame(39))
        ));
    }

    #[test]
    fn hello_messages_round_trip() {
        let client = ClientMessage {
            msg: Some(client_message::Msg::Hello(HelloFromClient {
                public_key: vec![7u8; 32],
                client_information: Some(ClientInformation {
                    name: "integratedeck".into(),
                    user_agent: "ideck/0.1".into(),
                    device_type: 2,
                    device_model: "software".into(),
                    os_name: "windows".into(),
                    os_version: "11".into(),
                }),
            })),
        };
        let decoded = ClientMessage::decode(client.encode_to_vec().as_slice()).unwrap();
        assert_eq!(client, decoded);

        let server = ServerMessage {
            msg: Some(server_message::Msg::Hello(HelloFromServer {
                workstation_id: "ws".into(),
                hostname: "host".into(),
                version: "7.6.0".into(),
                os_type: 3,
                public_key: vec![9u8; 32],
                client_needs_authentication: true,
                os_name: "windows".into(),
                os_version: "11".into(),
                service_endpoints: Some(ServiceEndpoints {
                    addresses: vec!["192.168.3.44".into()],
                    port: 28198,
                }),
            })),
        };
        let decoded = ServerMessage::decode(server.encode_to_vec().as_slice()).unwrap();
        assert_eq!(server, decoded);
    }
}
/// Parse a comma separated BCM pin list (`"17,27,22"`), ignoring blanks and
/// values that do not fit in a pin number. Used by the Raspberry Pi bridge.
pub fn parse_button_pins(value: &str) -> Vec<u8> {
    value
        .split(',')
        .filter_map(|entry| entry.trim().parse::<u8>().ok())
        .collect()
}

#[cfg(test)]
mod pin_tests {
    use super::parse_button_pins;

    #[test]
    fn parses_the_default_pin_list() {
        assert_eq!(
            parse_button_pins("17,27,22,23,24,25"),
            vec![17, 27, 22, 23, 24, 25]
        );
    }

    #[test]
    fn ignores_blanks_garbage_and_overflow() {
        assert_eq!(parse_button_pins(" 4 , ,5,abc,300"), vec![4, 5]);
        assert!(parse_button_pins("").is_empty());
    }
}
