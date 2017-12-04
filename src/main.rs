extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

mod clock;

use std::net;
use clock::{TemporalRelation, VectorClock};

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
    Ping,
    // maybe with state?
    Pong,
    // maybe with state?
    Quit,
    Update { state: Clock },
    // TODO message types for sending the clipboard
}

/// A Node represents a node in the overlay, specifically our own node.
struct Node {
    // Own ID
    // Own Endpoint
    // Default TTL for sending packets
    // TCP connections, possibly map<PeerID, TCP Connection>
    // Some storage for sent requests awaiting response, something like map<message ID, ???>,
    // possibly with a timeout?
    // OR make sending messages and receiving responses synchronous, i.e. whenever we send a
    // message, we have to wait for the response
    // Current clock
    // Some storage for received JoinRequests, to route the responses back
}

impl Node {
    // TCP accept loop: add new connections to Node, maybe ignore some if we have enough? Maybe not
    // now.

    // TCP receive loop: We could either do one loop and use select, or non-blocking IO, or do
    // multiple loops, one per TCP connection.
    // If we do multiple loops (= multiple threads), these threads will probably be the owner of
    // the TCP connection, i.e. we need a way to get data to them so they can send it on the TCP
    // connection.

    // Something like a send_to method:
    fn send_to(&self, msg: &Message) -> Result<u32, str> {
        // figure out destination address if we know it and are connected
        // figure out type of message and whether we need to wait for a response
        // if yes: Remember that somehow for later OR use the synchronous approach, i.e. block until
        // response received
        // serialize message
        // prepend message length in bytes
        // send on TCP connection
        // ez lyfe
        Ok(0)
    }
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
        //message_type: MessageType::JoinRequest { src_ep: Endpoint {
        // ip: net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1)), port: 12345 } },
        src_id: 1234,
        ttl: 7,
        hop_count: 0,
    };

    let serialized = serde_json::to_string(&msg).unwrap();

    println!("serialized = {}", serialized);

    let deserialized: Message = serde_json::from_str(&serialized).unwrap();

    println!("deserialized = {:?}", deserialized);
}
