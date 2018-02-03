use std::net;
use clock::VectorClock;
use std::error::Error;
use std::io::Read;
use std::io;
use std::net::TcpStream;
use std::net::SocketAddr;
use std::io::Write;
use std::str::FromStr;

use rand;
use rand::Rng;
use serde;
use serde_json;

use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};

pub type PeerID = Endpoint;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CopyClock {
    pub clock: VectorClock<PeerID>,
    pub last_copy_src: PeerID,
}

impl CopyClock {
    pub fn new(clock: &VectorClock<PeerID>, last_copy_src: &PeerID) -> CopyClock {
        CopyClock {
            clock: clock.clone(),
            last_copy_src: last_copy_src.clone(),
        }
    }
}

#[derive(Debug, Ord, PartialOrd, PartialEq, Eq, Hash, Copy, Clone)]
pub struct Endpoint {
    ip: net::Ipv4Addr,
    port: u16,
}

impl serde::Serialize for Endpoint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = format!("{}:{}", self.ip, self.port);
        serializer.serialize_str(&s)
    }
}

impl<'de> serde::Deserialize<'de> for Endpoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        // TODO maybe fix this
        let addr = SocketAddr::from_str(&s).unwrap();
        match addr {
            SocketAddr::V6(_) => panic!("deserialized IPv6 address for endpoint?"),
            SocketAddr::V4(v4) => Ok(Endpoint {
                ip: v4.ip().clone(),
                port: v4.port(),
            }),
        }
    }
}

impl Endpoint {
    pub fn new(ip: &net::Ipv4Addr, port: u16) -> Endpoint {
        Endpoint {
            ip: ip.clone(),
            port: port,
        }
    }
}

pub type MessageID = [u8; 16];

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub message_id: MessageID,
    pub message_type: MessageType,
    pub src_id: PeerID,
    pub ttl: u32,
    pub hop_count: u32,
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

#[derive(Serialize, Deserialize, Debug, Clone)]
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

fn write_length_prefixed(conn: &mut net::TcpStream, msg: &Message) -> Result<(), Box<Error>> {
    let serialized = serde_json::to_vec(&msg)?;
    let len = serialized.len() as u32;

    conn.write_u32::<BigEndian>(len)?;
    let n = conn.write(&serialized.to_vec())?;

    if n != serialized.len() {
        // TODO we _could_ handle this nicely
        return Err(From::from(format!(
            "Unable to send, wrote {} bytes, expected {}",
            n,
            serialized.len()
        )));
    }

    Ok(())
}

fn read_length_prefixed(r: &mut TcpStream) -> Result<Message, Box<Error>> {
    let len = r.read_u32::<BigEndian>()?;

    // we just need a slice of length len, how hard can it be...
    let mut buf = Vec::with_capacity(len as usize);
    for i in 0..len {
        buf.push(0);
    }

    r.read_exact(buf.as_mut_slice())?;

    let deserialized: Message = serde_json::from_slice(&buf)?;

    Ok(deserialized)
}

pub fn accept(socket: &mut net::TcpListener) -> Result<IncomingConnection, Box<Error>> {
    for conn in socket.incoming() {
        if let Err(err) = conn {
            return Err(From::from(err));
        }
        let mut stream = conn?;

        let deserialized = read_length_prefixed(&mut stream)?;

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
                println!(
                    "accept: Got invalid first message from {:?}: {:?}",
                    stream.peer_addr(),
                    deserialized
                );
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

// TODO this might not be needed later
impl Connection {
    pub fn read_message(&mut self) -> Result<Message, Box<Error>> {
        match self {
            &mut Connection::Join(ref mut c) => c.read_message(),
            &mut Connection::Copy(ref mut c) => c.read_message(),
            &mut Connection::P2P(ref mut c) => c.read_message(),
        }
    }
}

#[derive(Debug, PartialOrd, PartialEq, Clone)]
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
        let mut stream = TcpStream::connect(SocketAddr::from((remote.ip, remote.port)))?;

        write_length_prefixed(&mut stream, &msg)?;

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

    pub fn respond(&mut self, own_id: &Endpoint, incoming: &Message) -> Result<(), Box<Error>> {
        if self.dir != Direction::Incoming {
            return Err(From::from("can only respond on incoming connections"));
        }

        let msg = Message {
            message_id: incoming.message_id,
            message_type: MessageType::JoinResponse {
                target: incoming.src_id.clone(),
            },
            src_id: own_id.clone(),
            ttl: incoming.ttl,
            hop_count: incoming.hop_count,
        };

        write_length_prefixed(&mut self.conn, &msg)
    }

    pub fn forward_response(
        &mut self,
        incoming: &Message,
        target: &Endpoint,
    ) -> Result<(), Box<Error>> {
        if self.dir != Direction::Incoming {
            return Err(From::from("can only respond on incoming connections"));
        }

        let msg = Message {
            message_id: incoming.message_id,
            message_type: MessageType::JoinResponse {
                target: target.clone(),
            },
            src_id: incoming.src_id.clone(),
            ttl: incoming.ttl,
            hop_count: incoming.hop_count,
        };

        write_length_prefixed(&mut self.conn, &msg)
    }

    pub fn read_message(&mut self) -> Result<Message, Box<Error>> {
        if self.dir != Direction::Outgoing {
            return Err(From::from("can only read on outgoing JoinConnection"));
        }

        read_length_prefixed(&mut self.conn)
    }

    pub fn close(mut self) -> Result<(), Box<Error>> {
        self.conn.flush()?;
        Ok(())
        // conn should be closed as soon as it is dropped, i.e. here
    }
}

#[derive(Debug)]
pub struct CopyConnection {
    conn: net::TcpStream,
    dir: Direction,
}

impl CopyConnection {
    fn connect(remote: &PeerID, msg: Message) -> Result<CopyConnection, Box<Error>> {
        let mut stream = TcpStream::connect(SocketAddr::from((remote.ip, remote.port)))?;

        write_length_prefixed(&mut stream, &msg)?;

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

    pub fn respond(&mut self, text: &String, local: &Endpoint) -> Result<(), Box<Error>> {
        if self.dir != Direction::Incoming {
            return Err(From::from("can only respond on incoming connection"));
        }

        let msg = Message {
            message_id: generate_message_id(),
            message_type: MessageType::TextResponse { text: text.clone() },
            src_id: local.clone(),
            ttl: 1,
            hop_count: 0,
        };

        write_length_prefixed(&mut self.conn, &msg)
    }

    pub fn respond_error(
        &mut self,
        error: &String,
        state: &CopyClock,
        local: &Endpoint,
    ) -> Result<(), Box<Error>> {
        if self.dir != Direction::Incoming {
            return Err(From::from("can only respond on incoming connection"));
        }

        let msg = Message {
            message_id: generate_message_id(),
            message_type: MessageType::ErrorResponse {
                error: error.clone(),
                state: state.clone(),
            },
            src_id: local.clone(),
            ttl: 1,
            hop_count: 0,
        };

        write_length_prefixed(&mut self.conn, &msg)
    }

    pub fn read_message(&mut self) -> Result<Message, Box<Error>> {
        if self.dir != Direction::Outgoing {
            return Err(From::from("can only read on outgoing CopyConnection"));
        }

        read_length_prefixed(&mut self.conn)
    }

    pub fn close(mut self) -> Result<(), Box<Error>> {
        self.conn.flush()?;
        Ok(())
        // conn should be closed as soon as it is dropped, i.e. here
    }
}

#[derive(Debug)]
pub struct P2PConnection {
    conn: net::TcpStream,
    dir: Direction,
}

impl P2PConnection {
    fn connect(remote: &PeerID, msg: Message) -> Result<P2PConnection, Box<Error>> {
        let mut stream = TcpStream::connect(SocketAddr::from((remote.ip, remote.port)))?;

        write_length_prefixed(&mut stream, &msg)?;

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

    pub fn dup(&self) -> Result<P2PConnection, Box<Error>> {
        let conn = self.conn.try_clone()?;
        Ok(P2PConnection {
            conn: conn,
            dir: self.dir.clone(),
        })
    }

    pub fn ping(&mut self, state: &CopyClock, local: &Endpoint) -> Result<(), Box<Error>> {
        let msg = Message {
            message_id: generate_message_id(),
            message_type: MessageType::Ping {
                state: state.clone(),
            },
            src_id: local.clone(),
            ttl: 1,
            hop_count: 0,
        };

        write_length_prefixed(&mut self.conn, &msg)?;

        Ok(())
    }

    pub fn pong(&mut self, state: &CopyClock, local: &Endpoint) -> Result<(), Box<Error>> {
        let msg = Message {
            message_id: generate_message_id(),
            message_type: MessageType::Pong {
                state: state.clone(),
            },
            src_id: local.clone(),
            ttl: 1,
            hop_count: 0,
        };

        write_length_prefixed(&mut self.conn, &msg)?;

        Ok(())
    }

    pub fn notify_copy(&mut self, state: &CopyClock, local: &Endpoint) -> Result<(), Box<Error>> {
        let msg = Message {
            message_id: generate_message_id(),
            message_type: MessageType::CopyNotification {
                state: state.clone(),
            },
            src_id: local.clone(),
            ttl: 8,
            hop_count: 0,
        };

        write_length_prefixed(&mut self.conn, &msg)?;

        Ok(())
    }

    pub fn forward_notify_copy(
        &mut self,
        state: &CopyClock,
        local: &Endpoint,
        ttl: u32,
        hop_count: u32,
    ) -> Result<(), Box<Error>> {
        let msg = Message {
            message_id: generate_message_id(), // TODO reuse message_id from incoming message
            message_type: MessageType::CopyNotification {
                state: state.clone(),
            },
            src_id: local.clone(),
            ttl: ttl,
            hop_count: hop_count,
        };

        write_length_prefixed(&mut self.conn, &msg)?;

        Ok(())
    }

    pub fn read_message(&mut self) -> Result<Message, Box<Error>> {
        read_length_prefixed(&mut self.conn)
    }

    pub fn close(mut self) -> Result<(), Box<Error>> {
        self.conn.flush()?;
        Ok(())
        // conn should be closed as soon as it is dropped, i.e. here
    }
}
