#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;

mod clock;

use std::net;
use clock::{VectorClock, TemporalRelation};

type Clock = VectorClock<PeerID>;

type PeerID = u32;

#[derive(Serialize, Deserialize, Debug)]
struct Endpoint {
    ip: net::IpAddr,
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
    JoinRequest { src_ep: Endpoint },
    JoinResponse { peer_ep: Endpoint },
    Ping, // maybe with state?
    Pong, // maybe with state?
    Quit,
    Update { state: Clock },
    // TODO message types for sending the clipboard
}


fn main() {
    let mut c = Clock::new();
    //c = c.increment(1);
    //c = c.increment(2);
    //c = c.increment(1);
    c.incr(1);
    c.incr(2);
    c.incr(1);

    let msg = Message {
        message_id: [0; 16],
        message_type: MessageType::Update { state: c },
        //message_type: MessageType::JoinRequest { src_ep: Endpoint { ip: net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1)), port: 12345 } },
        src_id: 1234,
        ttl: 7,
        hop_count: 0
    };

    let serialized = serde_json::to_string(&msg).unwrap();

    println!("serialized = {}", serialized);

    let deserialized: Message = serde_json::from_str(&serialized).unwrap();

    println!("deserialized = {:?}", deserialized);
}

