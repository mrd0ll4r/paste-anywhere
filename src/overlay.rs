use std::sync::{Arc, Mutex};
use std::net::TcpListener;
use std::net::Ipv4Addr;
use std::error::Error;
use std::thread;

use network::*;

pub struct Overlay {
    own_id: PeerID,
    sock: Arc<Mutex<TcpListener>>,
    bootstrap_ids: Vec<PeerID>,
    available_ids: Mutex<Vec<PeerID>>,
}

impl Overlay {
    pub fn new(addr: &Ipv4Addr) -> Result<Overlay, Box<Error>> {
        let sock = TcpListener::bind((addr.clone(), 0 as u16))?;
        let local = sock.local_addr()?;

        Ok(Overlay {
            own_id: PeerID::new(&addr, local.port()),
            sock: Arc::new(Mutex::new(sock)),
            bootstrap_ids: Vec::new(),
            available_ids: Mutex::new(Vec::new()),
        })
    }

    fn perform_join_single(&self, mut conn: JoinConnection) {
        loop {
            let msg = conn.read_message();
            if let Err(e) = msg {
                println!("join: read failed, assuming connection closed: {:?}", e);
                conn.close();
                return;
            }

            let msg = msg.unwrap();

            if let MessageType::JoinResponse { target } = msg.message_type {
                if !target.eq(&self.own_id) {
                    println!(
                        "join: got wrong target, dropping connection. Got: {:?}",
                        msg
                    );
                    return;
                }

                let mut available = self.available_ids.lock().unwrap();
                available.push(msg.src_id);
            } else {
                println!(
                    "join: received wrong message type, dropping connection. Got: {:?}",
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
                    println!("join: opened a connection to {:?}", id);
                    self.perform_join_single(conn);
                }
                Err(e) => {
                    println!("join: unable to connect to {:?}: {}", id, e);
                    continue;
                }
            }
        }


        let mut available = self.available_ids.lock().unwrap();
        available.as_mut_slice().sort();
        available.dedup();
        println!("join: got these peers: {:?}", available);
        if available.len() == 0 {
            return Err(From::from("I know no peers"));
        }

        Ok(())
    }


    // TODO this static lifetime is probably gonna come back to haunt us
    pub fn start_accepting(&'static mut self) -> Result<(), Box<Error>> {
        thread::spawn(move || {
            let mut sock = self.sock.lock().unwrap();
            loop {
                let mut incoming = accept(&mut sock).unwrap();
                println!(
                    "Incoming connection: {:?}, first message: {:?}",
                    incoming.conn,
                    incoming.first_msg
                );

                match incoming.conn {
                    Connection::P2P(mut c) => {
                        thread::spawn(move || {
                            loop {
                                let msg = c.read_message();
                                match msg {
                                    Ok(m) => println!("received: {:?}", m),
                                    Err(e) => {
                                        println!("unable to read: {}", e);
                                        return;
                                    }
                                }
                            }
                        });
                    }
                    Connection::Copy(mut c) => {
                        thread::spawn(move || {
                            loop {
                                let msg = c.read_message();
                                match msg {
                                    Ok(m) => println!("received: {:?}", m),
                                    Err(e) => {
                                        println!("unable to read: {}", e);
                                        return;
                                    }
                                }
                            }
                        });
                    }
                    Connection::Join(mut c) => {
                        thread::spawn(move || {
                            loop {
                                let msg = c.read_message();
                                match msg {
                                    Ok(m) => println!("received: {:?}", m),
                                    Err(e) => {
                                        println!("unable to read: {}", e);
                                        return;
                                    }
                                }
                            }
                        });
                    }
                }
            }
        });

        Ok(())
    }
}
