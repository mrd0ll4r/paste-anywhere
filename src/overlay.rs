use std::sync::{Arc, Mutex};
use std::net::TcpListener;
use std::net::Ipv4Addr;
use std::error::Error;
use std::thread;
use std::collections::HashMap;
use std::sync::mpsc::*;
use std::time;

use clock::VectorClock;
use clock::TemporalRelation;
use network::*;

use rand;
use rand::Rng;

#[derive(Clone, Debug)]
enum P2PSend {
    Ping(CopyClock),
    CopyNotification(CopyClock),
    ForwardCopyNotification(CopyClock, u32, u32),
}

fn update_state(overlay_state: Arc<Mutex<CopyClock>>, new_state: CopyClock) -> CopyClock {
    let mut overlay_state = overlay_state.lock().unwrap();
    let ord = overlay_state.clock.temporal_relation(&new_state.clock);

    match ord {
        TemporalRelation::Equal => overlay_state.clone(),
        TemporalRelation::EffectOf => overlay_state.clone(),
        TemporalRelation::Caused => {
            *overlay_state = new_state;
            overlay_state.clone()
        }
        TemporalRelation::ConcurrentGreater => overlay_state.clone(),
        TemporalRelation::ConcurrentSmaller => {
            *overlay_state = new_state;
            overlay_state.clone()
        }
    }
}

#[derive(Debug)]
struct Peer {
    sender: SyncSender<P2PSend>,
    closer: SyncSender<()>,
}

impl Peer {
    fn new(
        mut conn: P2PConnection,
        own_id: PeerID,
        overlay_state: Arc<Mutex<CopyClock>>,
    ) -> Result<Peer, Box<Error>> {
        let mut conn2 = conn.dup()?;
        let (send_tx, send_rx) = sync_channel(0);
        let (close_tx, close_rx) = sync_channel(0);
        let close_copy = close_tx.clone();
        let send_copy = send_tx.clone();
        let id_copy = own_id.clone();

        thread::spawn(move || loop {
            let msg = conn.read_message();
            if let Err(e) = msg {
                println!("peer: unable to read, closing: {}", e);
                conn.close();
                close_copy.send(());
                println!("peer: closed");
                return;
            }
            let msg = msg.unwrap();
            // TODO maybe move this to another thread? I smell a deadlock from ping-ponging copy notifications...
            match msg.message_type {
                MessageType::Ping { state } => {
                    println!("peer: received ping with state: {:?}", state);
                    let new_state = update_state(overlay_state.clone(), state);
                    println!("peer: updated overlay state to {:?}", new_state);

                    println!("peer: replying with pong");
                    let resp = conn.pong(&new_state, &id_copy);
                    if let Err(e) = resp {
                        println!("peer: unable to reply, closing: {}", e);
                        conn.close();
                        close_copy.send(());
                        println!("peer: closed");
                        return;
                    }
                    println!("peer: reply successful");
                }
                MessageType::Pong { state } => {
                    println!("peer: received pong with state: {:?}", state);
                    let new_state = update_state(overlay_state.clone(), state);
                    println!("peer: updated overlay state to {:?}", new_state);
                }
                MessageType::CopyNotification { state } => {
                    println!(
                        "peer: received copy notification with state: {:?}, ttl: {}",
                        state, msg.ttl
                    );
                    // TODO implement me
                }
                _ => {
                    println!("peer: received invalid message: {:?}", msg);
                    conn.close();
                    close_copy.send(());
                    println!("peer: closed");
                    return;
                }
            }
        });

        thread::spawn(move || loop {
            select! {
                msg = send_rx.recv() => {
                    let msg = msg.unwrap();
                    println!("peer: received data to send: {:?}",msg);
                    match msg {
                        P2PSend::Ping(clock) => {
                            let resp = conn2.ping(&clock,&own_id);
                            if let Err(e) = resp {
                                println!("peer: unable to send, closing: {}",e);
                                conn2.close();
                                return
                            }
                        },
                        P2PSend::CopyNotification(clock) => {
                            let resp = conn2.notify_copy(&clock,&own_id);
                            if let Err(e) = resp {
                                println!("peer: unable to send, closing: {}",e);
                                conn2.close();
                                return
                            }
                        },
                        P2PSend::ForwardCopyNotification(clock,ttl,hop_count) => {
                            let resp = conn2.forward_notify_copy(&clock,&own_id,ttl,hop_count);
                            if let Err(e) = resp {
                                println!("peer: unable to send, closing: {}",e);
                                conn2.close();
                                return
                            }
                        }
                    }
                    println!("peer: send successful");
                },
                _ = close_rx.recv() => {
                    println!("peer: received close signal");
                    conn2.close();
                    return
                }
            }
        });

        Ok(Peer {
            sender: send_tx,
            closer: close_tx,
        })
    }

    fn ping(&self, state: CopyClock) -> Result<(), Box<Error>> {
        self.sender.send(P2PSend::Ping(state))?;
        Ok(())
    }

    fn notify_copy(&self, state: CopyClock) -> Result<(), Box<Error>> {
        self.sender.send(P2PSend::CopyNotification(state))?;
        Ok(())
    }

    fn forward_notify_copy(
        &self,
        state: CopyClock,
        ttl: u32,
        hop_count: u32,
    ) -> Result<(), Box<Error>> {
        self.sender
            .send(P2PSend::ForwardCopyNotification(state, ttl, hop_count))?;
        Ok(())
    }

    fn close(&self) -> Result<(), Box<Error>> {
        self.closer.send(())?;
        Ok(())
    }
}

pub struct Overlay {
    own_id: PeerID,
    sock: Arc<Mutex<TcpListener>>,
    bootstrap_ids: Vec<PeerID>,
    available_ids: Mutex<Vec<PeerID>>,
    connected_peers: Arc<Mutex<HashMap<PeerID, Peer>>>,
    // TODO have state, clipboard in one mutex
    state: Arc<Mutex<CopyClock>>,
    clipboard: Arc<Mutex<String>>,
    seen_join_message_ids: Arc<Mutex<HashMap<MessageID, ()>>>,
}

impl Overlay {
    pub fn new(addr: &Ipv4Addr, bootstrap_peers: Vec<PeerID>) -> Result<Overlay, Box<Error>> {
        let sock = TcpListener::bind((addr.clone(), 0 as u16))?;
        let local = sock.local_addr()?;
        println!("overlay: bound to address {}", local);

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

        let mut overlay_state = self.state.lock().unwrap();
        *overlay_state = CopyClock {
            clock: overlay_state.clock.incr_clone(self.own_id.clone()),
            last_copy_src: self.own_id.clone(),
        };
        let state = overlay_state.clone();
        println!("set_clipboard: set state to {:?}", state);
        drop(overlay_state);
        drop(current);

        // TODO send out copy notifications

        Ok(())
    }

    pub fn get_clipboard(&self) -> Result<String, Box<Error>> {
        let overlay_state = self.state.lock().unwrap().clone();
        if overlay_state.last_copy_src.eq(&self.own_id) {
            return Ok(self.clipboard.lock().unwrap().clone());
        }

        let mut conn = CopyConnection::open(
            &self.own_id,
            &overlay_state.last_copy_src,
            8,
            &"text".to_string(),
        )?;

        let msg = conn.read_message()?;

        if let MessageType::ErrorResponse { state, error } = msg.message_type {
            println!(
                "->copy: received error response, err: {}, state: {:?}",
                error, state
            );
            let new_state = update_state(self.state.clone(), state);
            println!("->copy: updated own state to {:?}", new_state);
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

    pub fn perform_join(&self) -> Result<(), Box<Error>> {
        // TODO make this  parallel
        // TODO only take n peers for this, not all of them
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
        println!("->join: got these peers: {:?}", *available);
        if available.len() == 0 {
            return Err(From::from("I know no peers"));
        }

        let state = self.state.lock().unwrap().clone();

        // Establish P2P connections with a bunch of peers
        {
            let mut peers = self.connected_peers.lock().unwrap();

            // TODO only take a bunch of peers, not all of them
            for i in 0..available.len() {
                let p = available[i];
                println!("->join: building p2p connection to peer at {:?}", p);
                let mut p2p_conn = P2PConnection::open(&self.own_id, &p, 8, &state);
                if let Err(e) = p2p_conn {
                    println!("->join: unable to open connection: {}", e);
                    continue;
                }
                let mut p2p_conn = p2p_conn.unwrap();
                let peer = Peer::new(p2p_conn, self.own_id.clone(), self.state.clone());
                if let Err(e) = peer {
                    println!("->join: unable to construct peer: {}", e);
                    continue;
                }
                let peer = peer.unwrap();
                peers.insert(p.clone(), peer);
                println!("->join: p2p connection successful");
            }

            if peers.len() == 0 {
                return Err(From::from("Not connected to any peers"));
            }
        }

        Ok(())
    }

    pub fn start_autoping(&self) {
        let peers = self.connected_peers.clone();
        let own_id = self.own_id.clone();
        let state = self.state.clone();
        thread::spawn(move || {
            thread::sleep_ms(rand::thread_rng().gen_range(1000, 5000));
            loop {
                {
                    let mut p: Vec<Endpoint> = Vec::new();
                    let mut peers_to_remove: Vec<Endpoint> = Vec::new();
                    let mut peers = peers.lock().unwrap();
                    for peer in peers.keys() {
                        p.push(peer.clone());
                    }
                    println!("ping: sending ping to these peers: {:?}", p);

                    for i in 0..p.len() {
                        let peer = peers.get(&p[i]).unwrap();

                        println!("ping: sending ping to {:?}", p[i]);
                        let resp = peer.ping(state.lock().unwrap().clone());
                        if let Err(e) = resp {
                            println!("ping: unable to send, removing peer: {}", e);
                            peer.close();
                            peers_to_remove.push(p[i].clone());
                            continue;
                        }
                        println!("ping: ping successful");
                    }

                    for i in 0..peers_to_remove.len() {
                        peers.remove(&peers_to_remove[i]);
                    }
                }

                println!("ping: done pinging all peers, sleeping");

                thread::sleep(time::Duration::new(10, 0));
            }
        });
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
                        Overlay::handle_p2p_connection(
                            c,
                            own_id.clone(),
                            incoming.first_msg.src_id.clone(),
                            peers.clone(),
                            state.clone(),
                        );
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

    fn handle_p2p_connection(
        mut c: P2PConnection,
        own_id: PeerID,
        remote_id: PeerID,
        peers: Arc<Mutex<HashMap<Endpoint, Peer>>>,
        state: Arc<Mutex<CopyClock>>,
    ) {
        thread::spawn(move || {
            // TODO update state
            let peer = Peer::new(c, own_id, state.clone());
            if let Err(e) = peer {
                println!("<-p2p: unable to construct peer: {}", e);
                return;
            }
            let peer = peer.unwrap();

            let mut peers = peers.lock().unwrap();
            peers.insert(remote_id, peer);
        });
    }

    fn handle_join_connection(
        mut c: JoinConnection,
        peers: Arc<Mutex<HashMap<PeerID, Peer>>>,
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

            // TODO make this parallel
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

            println!("<-join: done forwarding, responding with own ID");
            let resp = c.respond(&own_id, &msg);
            match resp {
                Ok(_) => println!("<-join: reply successful"),
                Err(e) => println!("<-join: unable to reply: {}", e),
            }
            c.close();
        });
    }
}
