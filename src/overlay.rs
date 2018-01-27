use std::sync::{Arc, Mutex};
use std::net::TcpListener;
use std::net::Ipv4Addr;
use std::error::Error;
use std::thread;
use std::collections::HashMap;

use clock::VectorClock;
use network::*;

pub struct Overlay {
    own_id: PeerID,
    sock: Arc<Mutex<TcpListener>>,
    bootstrap_ids: Vec<PeerID>,
    available_ids: Mutex<Vec<PeerID>>,
    connected_peers: Arc<Mutex<HashMap<PeerID, ()>>>,
    // TODO have state, clipboard in one mutex
    state: Arc<Mutex<CopyClock>>,
    clipboard: Arc<Mutex<String>>,
    seen_join_message_ids: Arc<Mutex<HashMap<MessageID, ()>>>,
}

impl Overlay {
    pub fn new(addr: &Ipv4Addr, bootstrap_peers: Vec<PeerID>) -> Result<Overlay, Box<Error>> {
        let sock = TcpListener::bind((addr.clone(), 0 as u16))?;
        let local = sock.local_addr()?;

        Ok(Overlay {
            own_id: PeerID::new(&addr, local.port()),
            sock: Arc::new(Mutex::new(sock)),
            bootstrap_ids: bootstrap_peers,
            available_ids: Mutex::new(Vec::new()),
            connected_peers: Arc::new(Mutex::new(HashMap::new())),
            state: Arc::new(Mutex::new(CopyClock::new(
                &VectorClock::new(),
                &PeerID::new(&addr, local.port()),
            ))),
            clipboard: Arc::new(Mutex::new(String::new())),
            seen_join_message_ids: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn set_clipboard(&self, clipboard: &String) -> Result<(), Box<Error>> {
        let mut current = self.clipboard.lock().unwrap();
        *current = clipboard.clone();

        // TODO increment state
        // TODO send out copy notifications

        Ok(())
    }

    pub fn get_clipboard(&self) -> Result<String, Box<Error>> {
        let state = self.state.lock().unwrap().clone();
        if state.last_copy_src.eq(&self.own_id) {
            return Ok(self.clipboard.lock().unwrap().clone());
        }

        let mut conn =
            CopyConnection::open(&self.own_id, &state.last_copy_src, 8, &"text".to_string())?;

        let msg = conn.read_message()?;

        if let MessageType::ErrorResponse { state, error } = msg.message_type {
            // TODO update state
            return Err(From::from(format!("remote  replied with error: {}", error)));
        }
        if let MessageType::TextResponse { text } = msg.message_type {
            return Ok(text);
        }

        println!("->copy: received invalid response, got: {:?}", msg);
        Err(From::from("remote sent an invalid reply, check logs"))
    }

    fn perform_join_single(&self, mut conn: JoinConnection) {
        loop {
            let msg = conn.read_message();
            if let Err(e) = msg {
                println!("->join: unable to read: {}, assuming connection closed", e);
                conn.close();
                return;
            }

            let msg = msg.unwrap();

            if let MessageType::JoinResponse { target } = msg.message_type {
                if !target.eq(&self.own_id) {
                    println!(
                        "->join: got wrong target, dropping connection. Got: {:?}",
                        msg
                    );
                    return;
                }

                let mut available = self.available_ids.lock().unwrap();
                available.push(msg.src_id);
            } else {
                println!(
                    "->join: received wrong message type, dropping connection. Got: {:?}",
                    msg
                );
                return;
            }
        }
    }

    pub fn perform_join(&mut self) -> Result<(), Box<Error>> {
        for i in 0..self.bootstrap_ids.len() {
            let id = self.bootstrap_ids.get(i).unwrap();

            let mut join_conn = JoinConnection::open(&self.own_id, id, 8);
            match join_conn {
                Ok(mut conn) => {
                    println!("->join: opened a connection to {:?}", id);
                    self.perform_join_single(conn);
                }
                Err(e) => {
                    println!("->join: unable to connect to {:?}: {}", id, e);
                    continue;
                }
            }
        }

        let mut available = self.available_ids.lock().unwrap();
        available.as_mut_slice().sort();
        available.dedup();
        println!("->join: got these peers: {:?}", available);
        if available.len() == 0 {
            return Err(From::from("I know no peers"));
        }

        // TODO establish P2P connections with these peers

        Ok(())
    }

    pub fn start_accepting(&self) {
        let s = self.sock.clone();
        let peers = self.connected_peers.clone();
        let own_id = self.own_id.clone();
        let state = self.state.clone();
        let clipboard = self.clipboard.clone();
        let seen_message_ids = self.seen_join_message_ids.clone();
        thread::spawn(move || {
            let mut sock = s.lock().unwrap();
            loop {
                let mut incoming = accept(&mut sock).unwrap();
                println!(
                    "Incoming connection: {:?}, first message: {:?}",
                    incoming.conn, incoming.first_msg
                );

                match incoming.conn {
                    Connection::P2P(mut c) => {
                        Overlay::handle_p2p_connection(c, own_id.clone(), state.clone());
                    }
                    Connection::Copy(mut c) => {
                        Overlay::handle_copy_connection(
                            c,
                            own_id.clone(),
                            state.clone(),
                            clipboard.clone(),
                        );
                    }
                    Connection::Join(mut c) => {
                        Overlay::handle_join_connection(
                            c,
                            peers.clone(),
                            own_id.clone(),
                            incoming.first_msg.clone(),
                            seen_message_ids.clone(),
                        );
                    }
                }
            }
        });
    }

    fn handle_copy_connection(
        mut c: CopyConnection,
        own_id: PeerID,
        state: Arc<Mutex<CopyClock>>,
        clipboard: Arc<Mutex<String>>,
    ) {
        thread::spawn(move || {
            let state_copy = state.lock().unwrap().clone();

            if !state_copy.last_copy_src.eq(&own_id) {
                println!("<-copy: I don't have the latest clipboard, replying error");
                let resp = c.respond_error(
                    &"I don't have the latest clipboard".to_string(),
                    &state_copy,
                    &own_id,
                );
                match resp {
                    Ok(_) => println!("<-copy: reply successful"),
                    Err(e) => println!("<-copy: unable to reply: {}", e),
                }

                c.close();
                return;
            }

            let clipboard_copy = clipboard.lock().unwrap().clone();
            println!("<-copy: sending TextResponse...");
            let resp = c.respond(&clipboard_copy, &own_id);
            match resp {
                Ok(_) => println!("<-copy: reply successful"),
                Err(e) => println!("<-copy: unable to reply: {}", e),
            }

            c.close();
        });
    }

    fn handle_p2p_connection(mut c: P2PConnection, own_id: PeerID, state: Arc<Mutex<CopyClock>>) {
        thread::spawn(move || loop {
            let msg = c.read_message();
            match msg {
                Ok(m) => println!("received: {:?}", m),
                Err(e) => {
                    println!("unable to read: {}", e);
                    return;
                }
            }
        });
    }

    fn handle_join_connection(
        mut c: JoinConnection,
        peers: Arc<Mutex<HashMap<PeerID, ()>>>,
        own_id: PeerID,
        msg: Message,
        seen_message_ids: Arc<Mutex<HashMap<MessageID, ()>>>,
    ) {
        thread::spawn(move || {
            {
                let mut seen_message_ids = seen_message_ids.lock().unwrap();
                if seen_message_ids.contains_key(&msg.message_id) {
                    println!("<-join: I already saw this message ID, closing connection");
                    c.close();
                    return;
                }
                seen_message_ids.insert(msg.message_id, ());
            }

            if msg.ttl <= 1 {
                println!(
                    "<-join: msg.ttl is {}, will just reply and close connection",
                    msg.ttl
                );
                let resp = c.respond(&own_id, &msg);
                match resp {
                    Ok(_) => println!("<-join: reply successful"),
                    Err(e) => println!("<-join: unable to reply: {}", e),
                }
                c.close();
                return;
            }

            let mut p: Vec<Endpoint> = Vec::new();
            {
                let peers = peers.lock().unwrap();
                for peer in peers.keys() {
                    p.push(peer.clone());
                }
            }
            println!(
                "<-join: message has ttl={}, will forward to these peers: {:?}",
                msg.ttl, p
            );

            for ep in p.iter() {
                println!("<-join: forwarding to {:?}", ep);
                let mut conn = JoinConnection::forward(&ep, &msg);
                if let Err(e) = conn {
                    println!("<-join: unable to forward: {}", e);
                    continue;
                }
                let mut conn = conn.unwrap();

                loop {
                    let msg = conn.read_message();
                    if let Err(e) = msg {
                        println!("<-join: unable to read: {}", e);
                        conn.close();
                        break;
                    }
                    let msg = msg.unwrap();
                    if let MessageType::JoinResponse { target } = msg.message_type {
                        let resp = c.forward_response(&msg, &target);
                        match resp {
                            Ok(_) => println!(
                                "<-join: forwarded peer {:?} to peer {:?}",
                                msg.src_id, target
                            ),
                            Err(e) => {
                                println!("<-join: unable to forward: {}, closing", e);
                                c.close();
                                return;
                            }
                        }
                    } else {
                        println!("<-join: did not receive JoinResponse, got: {:?}", msg);
                        conn.close();
                        break;
                    }
                }
            }

            println!("<-join: done forwarding, closing");
            c.close();
        });
    }
}
