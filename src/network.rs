use std::net;
use clock::VectorClock;
use std::error::Error;
use std::io::Read;
use std::io;
use std::net::TcpStream;
use std::net::SocketAddr;
use std::io::Write;

use rand;
use rand::Rng;
use serde;
use serde_json;

pub type PeerID = Endpoint;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CopyClock {
    clock: VectorClock<PeerID>,
    last_copy_src: PeerID,
}

impl CopyClock {
    pub fn new(clock: &VectorClock<PeerID>, last_copy_src: &PeerID) -> CopyClock {
        CopyClock {
            clock: clock.clone(),
            last_copy_src: last_copy_src.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Ord, PartialOrd, PartialEq, Eq, Hash, Copy, Clone)]
pub struct Endpoint {
    ip: net::Ipv4Addr,
    port: u16,
}

impl Endpoint {
    pub fn new(ip: &net::Ipv4Addr, port: u16) -> Endpoint {
        Endpoint {
            ip: ip.clone(),
            port: port,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
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
pub enum MessageType {
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

pub fn accept(socket: &mut net::TcpListener) -> Result<IncomingConnection, Box<Error>> {
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

pub struct IncomingConnection {
    pub conn: Connection,
    pub first_msg: Message,
}

#[derive(Debug)]
pub enum Connection {
    Join(JoinConnection),
    Copy(CopyConnection),
    P2P(P2PConnection),
}

#[derive(Debug)]
pub enum Direction {
    Incoming,
    Outgoing,
}

#[derive(Debug)]
pub struct JoinConnection {
    conn: net::TcpStream,
    dir: Direction,
}

impl JoinConnection {
    fn connect(remote: &PeerID, msg: Message) -> Result<JoinConnection, Box<Error>> {
        let serialized = serde_json::to_string(&msg)?;

        let mut stream = TcpStream::connect(SocketAddr::from((remote.ip, remote.port)))?;

        stream.write(serialized.as_bytes())?;

        Ok(JoinConnection {
            conn: stream,
            dir: Direction::Outgoing,
        })
    }

    pub fn open(local: &PeerID, remote: &PeerID, ttl: u32) -> Result<JoinConnection, Box<Error>> {
        let msg = Message {
            message_id: generate_message_id(),
            message_type: MessageType::JoinRequest,
            src_id: local.clone(),
            ttl: ttl,
            hop_count: 0,
        };

        JoinConnection::connect(remote, msg)
    }

    pub fn forward(remote: &PeerID, incoming: &Message) -> Result<JoinConnection, Box<Error>> {
        let msg = Message {
            message_id: incoming.message_id,
            message_type: MessageType::JoinRequest,
            src_id: incoming.src_id.clone(),
            ttl: incoming.ttl - 1,
            hop_count: incoming.hop_count + 1,
        };

        JoinConnection::connect(remote, msg)
    }
}

#[derive(Debug)]
pub struct CopyConnection {
    conn: net::TcpStream,
    dir: Direction,
}

impl CopyConnection {
    fn connect(remote: &PeerID, msg: Message) -> Result<CopyConnection, Box<Error>> {
        let serialized = serde_json::to_string(&msg)?;

        let mut stream = TcpStream::connect(SocketAddr::from((remote.ip, remote.port)))?;

        stream.write(serialized.as_bytes())?;

        Ok(CopyConnection {
            conn: stream,
            dir: Direction::Outgoing,
        })
    }

    pub fn open(
        local: &PeerID,
        remote: &PeerID,
        ttl: u32,
        content_type: &String,
    ) -> Result<CopyConnection, Box<Error>> {
        let msg = Message {
            message_id: generate_message_id(),
            message_type: MessageType::CopyRequest {
                content_type: content_type.clone(),
            },
            src_id: local.clone(),
            ttl: ttl,
            hop_count: 0,
        };

        CopyConnection::connect(remote, msg)
    }
}

#[derive(Debug)]
pub struct P2PConnection {
    conn: net::TcpStream,
    dir: Direction,
}

impl P2PConnection {
    fn connect(remote: &PeerID, msg: Message) -> Result<P2PConnection, Box<Error>> {
        let serialized = serde_json::to_string(&msg)?;

        let mut stream = TcpStream::connect(SocketAddr::from((remote.ip, remote.port)))?;

        stream.write(serialized.as_bytes())?;

        Ok(P2PConnection {
            conn: stream,
            dir: Direction::Outgoing,
        })
    }

    pub fn open(
        local: &PeerID,
        remote: &PeerID,
        ttl: u32,
        state: &CopyClock,
    ) -> Result<P2PConnection, Box<Error>> {
        let msg = Message {
            message_id: generate_message_id(),
            message_type: MessageType::Ping {
                state: state.clone(),
            },
            src_id: local.clone(),
            ttl: ttl,
            hop_count: 0,
        };

        P2PConnection::connect(remote, msg)
    }
}
