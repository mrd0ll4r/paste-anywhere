extern crate serde;

use std::net;
use clock::VectorClock;

type PeerID = Endpoint;

#[derive(Serialize, Deserialize, Debug)]
struct CopyClock {
    clock: VectorClock<PeerID>,
    last_copy_src: PeerID,
}

#[derive(Serialize, Deserialize, Debug, Ord, PartialOrd, PartialEq, Eq, Hash)]
struct Endpoint {
    ip: net::Ipv4Addr,
    port: u16,
}

#[derive(Serialize, Deserialize, Debug)]
struct Message {
    message_id: [u8; 16],
    message_type: MessageType,
    src_id: PeerID,
    ttl: u32,
    hop_count: u32,
}

#[derive(Serialize, Deserialize, Debug)]
enum MessageType {
    JoinRequest, // use src_id as source
    JoinResponse { target: Endpoint },
    Ping { state: CopyClock },
    Pong { state: CopyClock },
    CopyNotification { state: CopyClock },
    CopyRequest { content_type: String },
    TextResponse { text: String },
    ErrorResponse { state: CopyClock, error: String },
}
