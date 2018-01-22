extern crate byteorder;
extern crate rand;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

mod clock;
mod network;
mod overlay;

use clock::*;
use network::*;
use overlay::*;

use std::net::*;
use std::env;
use std::thread;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        println!("Gonna be a client...");
        client(8080);
    } else {
        println!("Gonna be a server...");
        server();
    }
}

fn server() {
    let host = "127.0.0.1";
    let port = 8080;

    let mut sock = TcpListener::bind((host, port)).unwrap();
    println!("Listening..");

    loop {
        let mut incoming = network::accept(&mut sock).unwrap();
        println!("Got connection!");

        println!("{:?}", incoming.first_msg);
        println!("{:?}", incoming.conn);

        thread::spawn(move || {
            loop {
                let msg = incoming.conn.read_message();
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

fn client(port: u16) {
    let state = CopyClock::new(
        &VectorClock::new(),
        &PeerID::new(&Ipv4Addr::new(127, 0, 0, 1), 12345),
    );

    let local = PeerID::new(&Ipv4Addr::new(127, 0, 0, 1), 12345);
    let remote = PeerID::new(&Ipv4Addr::new(127, 0, 0, 1), port);

    let mut join_conn = JoinConnection::open(&local, &remote, 8).unwrap();
    println!("Connected Join...");
    join_conn.close().unwrap();
    println!("Closed Join...");

    thread::sleep_ms(500);

    let mut p2p_conn = P2PConnection::open(&local, &remote, 8, &state).unwrap();
    println!("Connected p2p...");
    p2p_conn.notify_copy(&state, &local).unwrap();
    p2p_conn.ping(&state, &local).unwrap();
    p2p_conn.pong(&state, &local).unwrap();
    p2p_conn.close().unwrap();
    println!("Closed p2p...");

    thread::sleep_ms(500);

    let mut copy_conn = CopyConnection::open(&local, &remote, 8, &"text".to_string()).unwrap();
    println!("Connected Copy...");
    copy_conn.close().unwrap();
    println!("Closed copy...");


    println!("Done!")
}
