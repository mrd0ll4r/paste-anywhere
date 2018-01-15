use std::net;
use clock::VectorClock;
use std::error::Error;
use std::io::Read;
use std::io;

use rand;
use rand::Rng;
use serde;
use serde_json;

type PeerID = Endpoint;

#[derive(Serialize, Deserialize, Debug)]
struct CopyClock {
    clock: VectorClock<PeerID>,
    last_copy_src: PeerID,
}

#[derive(Serialize, Deserialize, Debug, Ord, PartialOrd, PartialEq, Eq, Hash, Copy, Clone)]
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

fn generate_message_id() -> [u8; 16] {
    let mut b = [0u8; 16];

    let mut i = 0;
    for bb in rand::thread_rng().gen_iter::<u8>().take(16) {
        b[i] = bb;
        i = i + 1;
    }

    b
}

#[derive(Serialize, Deserialize, Debug)]
enum MessageType {
    // use src_id as source
    JoinRequest,
    JoinResponse { target: Endpoint },
    Ping { state: CopyClock },
    Pong { state: CopyClock },
    CopyNotification { state: CopyClock },
    CopyRequest { content_type: String },
    TextResponse { text: String },
    ErrorResponse { state: CopyClock, error: String },
}

fn accept(socket: &mut net::TcpListener) -> Result<IncomingConnection, Box<Error>> {
    for conn in socket.incoming() {
        if let Err(err) = conn {
            return Err(From::from(err));
        }
        let mut stream = conn.unwrap();

        let mut buf = [0u8; 1024];
        let n = stream.read(&mut buf)?;
        if n == 0 {
            return Err(From::from("Empty read, fixme maybe?"));
        }

        let deserialized: Message = serde_json::from_slice(&buf[0..n])?;

        match deserialized.message_type {
            MessageType::JoinRequest => {
                return Ok(IncomingConnection {
                    conn: Connection::Join(JoinConnection {
                        conn: stream,
                        dir: Direction::Incoming,
                    }),
                    first_msg: deserialized,
                });
            }
            MessageType::Ping { state: _ } => {
                return Ok(IncomingConnection {
                    conn: Connection::P2P(P2PConnection {
                        conn: stream,
                        dir: Direction::Incoming,
                    }),
                    first_msg: deserialized,
                });
            }
            MessageType::CopyRequest { content_type: _ } => {
                return Ok(IncomingConnection {
                    conn: Connection::Copy(CopyConnection {
                        conn: stream,
                        dir: Direction::Incoming,
                    }),
                    first_msg: deserialized,
                });
            }
            _ => {
                // TODO log this
                continue;
            }
        }
    }
    Err(From::from("no incoming connection?"))
}

struct IncomingConnection {
    conn: Connection,
    first_msg: Message,
}

enum Connection {
    Join(JoinConnection),
    Copy(CopyConnection),
    P2P(P2PConnection),
}

enum Direction {
    Incoming,
    Outgoing,
}

struct JoinConnection {
    conn: net::TcpStream,
    dir: Direction,
}
/*
impl JoinConnection {
    pub fn new() -> JoinConnection {}
}*/

struct CopyConnection {
    conn: net::TcpStream,
    dir: Direction,
}

/*
impl CopyConnection {
    pub fn new() -> CopyConnection {}
}
*/

struct P2PConnection {
    conn: net::TcpStream,
    dir: Direction,
}

/*
impl P2PConnection {
    pub fn new() -> P2PConnection {}
}
*/
