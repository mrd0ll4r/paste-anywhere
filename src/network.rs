use std::net;
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

use clock::VectorClock;

/// A PeerID is just an Endpoint.
pub type PeerID = Endpoint;

/// An Endpoint is a tuple of IPv4 address and TCP port.
/// It serializes to a string representation so it can be used as a key for JSON maps.
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

/// A CopyClock encapsulates a VectorClock and the PeerID of the peer who last pressed copy.
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

/// A MessageID is a 16-byte ID for a message, assumed to be unique.
pub type MessageID = [u8; 16];

fn generate_message_id() -> [u8; 16] {
    let mut b = [0u8; 16];

    let mut i = 0;
    for bb in rand::thread_rng().gen_iter::<u8>().take(16) {
        b[i] = bb;
        i = i + 1;
    }

    b
}

/// A Message is sent between two peers.
/// Every message has at least an ID, a source, a TTL and a hop count.
/// Different message types have additional content.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub message_id: MessageID,
    pub message_type: MessageType,
    pub src_id: PeerID,
    pub ttl: u32,
    pub hop_count: u32,
}

/// A MessageType encodes the type of a message and all fields specific to that type.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MessageType {
    /// A JoinRequest is propagated through the network without changing its source ID.
    /// That way, peers along the way know the ID (and by that the endpoint) of the new peer.
    /// A JoinRequest is the first message sent on a `JoinConnection`.
    JoinRequest,

    /// A JoinResponse is reverse-path routed to the joining peer.
    /// Its source ID is not modified during forwarding, so that the new peer knows the IDs (and by
    /// that the endpoints) of existing peers.
    JoinResponse { target: Endpoint },

    /// A Ping is used for the soft-state protocol.
    /// It contains the current state of the sending peer.
    /// Pings are not flooded through the network, but just ping-pong between two peers on a regular
    /// basis.
    /// A Ping is the first message sent on a `P2PConnection`.
    Ping { state: CopyClock },

    /// A Pong is the reply to a Ping.
    /// The peer receiving the Ping updates its state if necessary and returns its own state with a
    /// Pong.
    Pong { state: CopyClock },

    /// A CopyNotification is flooded through the network from the peer who pressed copy.
    CopyNotification { state: CopyClock },

    /// A CopyRequest is sent from a peer who pressed paste to the peer who last pressed copy.
    /// This is the first message sent on a `CopyConnection`, which is specifically opened between
    /// the two peers to exchange the clipboard.
    CopyRequest { content_type: String },

    /// A TextResponse is the response sent to a CopyRequest if the requested peer has the latest
    /// clipboard with text content type.
    TextResponse { text: String },

    /// An ErrorResponse is sent in response to a CopyRequest if the requested peer does not have
    /// the latest clipboard or not a textual clipboard.
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

/// Starts accepting incoming connections on the given socket, returning an IncomingConnection
/// on success.
/// This function determines the type of the incoming connection by its first message, which is
/// returned as part of the IncomingConnection.
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

/// An IncomingConnection encapsulates a Connection and the first message received.
pub struct IncomingConnection {
    pub conn: Connection,
    pub first_msg: Message,
}

/// A Connection determines the type of connection established between two peers.
#[derive(Debug)]
pub enum Connection {
    Join(JoinConnection),
    Copy(CopyConnection),
    P2P(P2PConnection),
}

/// The Direction determines the direction of a JoinConnection or CopyConnection.
/// P2PConnections are bidirectional by nature.
#[derive(Debug, PartialOrd, PartialEq, Clone)]
pub enum Direction {
    Incoming,
    Outgoing,
}

/// A JoinConnection is the type of connection established when a peer joins the network or searches
/// for more peers.
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

    /// Opens a new connection to [remote], presenting [local] as the joining peer.
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

    /// Opens a new connection to [remote], forwarding [incoming] as part of the flooding procedure.
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

    /// Responds to [incoming] with [own_id] as the ID.
    /// The response is then reverse-path routed to the original sender.
    pub fn respond(&mut self, own_id: &PeerID, incoming: &Message) -> Result<(), Box<Error>> {
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

    /// Forwards a response via reverse-path routing to [target].
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

    /// Reads a message off the underlying socket, iff this is an outgoing connection.
    pub fn read_message(&mut self) -> Result<Message, Box<Error>> {
        if self.dir != Direction::Outgoing {
            return Err(From::from("can only read on outgoing JoinConnection"));
        }

        read_length_prefixed(&mut self.conn)
    }

    /// Flushes and closes the connection.
    pub fn close(mut self) -> Result<(), Box<Error>> {
        self.conn.flush()?;
        Ok(())
        // conn should be closed as soon as it is dropped, i.e. here
    }
}

/// A CopyConnection is the type of connection established between a peer who wants the clipboard
/// and a peer who is believed to have the clipboard.
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

    /// Opens a new CopyConnection to [remote].
    pub fn open(
        local: &PeerID,
        remote: &PeerID,
        content_type: &String,
    ) -> Result<CopyConnection, Box<Error>> {
        let msg = Message {
            message_id: generate_message_id(),
            message_type: MessageType::CopyRequest {
                content_type: content_type.clone(),
            },
            src_id: local.clone(),
            ttl: 1,
            hop_count: 0,
        };

        CopyConnection::connect(remote, msg)
    }

    /// Responds to the request with the contents of the clipboard.
    pub fn respond(&mut self, text: &String, local: &PeerID) -> Result<(), Box<Error>> {
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

    /// Responds to the request with an error and the local state.
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

    /// Reads a message (the response) off the underlying socket, iff this is an outgoing
    /// connection.
    pub fn read_message(&mut self) -> Result<Message, Box<Error>> {
        if self.dir != Direction::Outgoing {
            return Err(From::from("can only read on outgoing CopyConnection"));
        }

        read_length_prefixed(&mut self.conn)
    }

    /// Flushes and closes the connection.
    pub fn close(mut self) -> Result<(), Box<Error>> {
        self.conn.flush()?;
        Ok(())
        // conn should be closed as soon as it is dropped, i.e. here
    }
}

/// A P2PConnection is the type of connection upheld between peers to exchange copy notifications
/// and soft state updates.
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

    /// Opens a new connection to [remote], sending a Ping with state [state].
    pub fn open(
        local: &PeerID,
        remote: &PeerID,
        state: &CopyClock,
    ) -> Result<P2PConnection, Box<Error>> {
        let msg = Message {
            message_id: generate_message_id(),
            message_type: MessageType::Ping {
                state: state.clone(),
            },
            src_id: local.clone(),
            ttl: 1,
            hop_count: 0,
        };

        P2PConnection::connect(remote, msg)
    }

    /// Attempts to duplicate the underlying socket.
    /// This is necessary if one thread is to read off the connection and another thread is to
    /// write.
    /// Behaviour is operating system dependent, but works on Linux...
    pub fn dup(&self) -> Result<P2PConnection, Box<Error>> {
        let conn = self.conn.try_clone()?;
        Ok(P2PConnection {
            conn: conn,
            dir: self.dir.clone(),
        })
    }

    /// Sends a Ping with state [state].
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

    /// Sends a Pong with state [state].
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

    /// Sends a CopyNotification with state [state] and TTL=8.
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

    /// Forwards a CopyNotification for flooding.
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

    /// Reads a message off the underlying socket.
    pub fn read_message(&mut self) -> Result<Message, Box<Error>> {
        read_length_prefixed(&mut self.conn)
    }

    /// Flushes and closes the connection.
    pub fn close(mut self) -> Result<(), Box<Error>> {
        self.conn.flush()?;
        Ok(())
        // conn should be closed as soon as it is dropped, i.e. here
    }
}
