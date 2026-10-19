//! VSD2 protobuf messages inferred from the 7.6.0.23012 binary and captures.

use prost::Message;

#[derive(Clone, PartialEq, Message)]
pub struct ClientMessage {
    #[prost(oneof = "client_message::Msg", tags = "1, 2")]
    pub msg: Option<client_message::Msg>,
}

pub mod client_message {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Msg {
        #[prost(message, tag = "1")]
        Hello(super::HelloFromClient),
        #[prost(message, tag = "2")]
        Authenticate(super::Authenticate),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct HelloFromClient {
    #[prost(bytes = "vec", tag = "1")]
    pub public_key: Vec<u8>,
    #[prost(message, optional, tag = "2")]
    pub client_information: Option<ClientInformation>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ClientInformation {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub user_agent: String,
    #[prost(int32, tag = "3")]
    pub device_type: i32,
    #[prost(string, tag = "4")]
    pub device_model: String,
    #[prost(string, tag = "5")]
    pub os_name: String,
    #[prost(string, tag = "6")]
    pub os_version: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct Authenticate {
    #[prost(string, tag = "1")]
    pub request_id: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct ServerMessage {
    #[prost(oneof = "server_message::Msg", tags = "10")]
    pub msg: Option<server_message::Msg>,
}

pub mod server_message {
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Msg {
        #[prost(message, tag = "10")]
        Hello(super::HelloFromServer),
    }
}

#[derive(Clone, PartialEq, Message)]
pub struct HelloFromServer {
    #[prost(string, tag = "1")]
    pub workstation_id: String,
    #[prost(string, tag = "2")]
    pub hostname: String,
    #[prost(string, tag = "3")]
    pub version: String,
    #[prost(int32, tag = "4")]
    pub os_type: i32,
    #[prost(bytes = "vec", tag = "5")]
    pub public_key: Vec<u8>,
    #[prost(bool, tag = "6")]
    pub client_needs_authentication: bool,
    #[prost(string, tag = "7")]
    pub os_name: String,
    #[prost(string, tag = "8")]
    pub os_version: String,
    #[prost(message, optional, tag = "9")]
    pub service_endpoints: Option<ServiceEndpoints>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ServiceEndpoints {
    #[prost(string, repeated, tag = "1")]
    pub addresses: Vec<String>,
    #[prost(uint32, tag = "2")]
    pub port: u32,
}
