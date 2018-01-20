extern crate rand;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

mod clock;
mod network;

use clock::*;
use network::*;

use std::net::*;
use std::env;

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
        let incoming = network::accept(&mut sock).unwrap();
        println!("Got connection!");

        println!("{:?}", incoming.first_msg);
        println!("{:?}", incoming.conn);
    }
}

fn client(port: u16) {
    let state = CopyClock::new(
        &VectorClock::new(),
        &PeerID::new(&Ipv4Addr::new(127, 0, 0, 1), 12345),
    );

    let local = PeerID::new(&Ipv4Addr::new(127, 0, 0, 1), 12345);
    let remote = PeerID::new(&Ipv4Addr::new(127, 0, 0, 1), port);

    let mut p2p_conn = P2PConnection::open(&local, &remote, 8, &state).unwrap();
    let mut copy_conn = CopyConnection::open(&local, &remote, 8, &"text".to_string());
    let mut join_conn = JoinConnection::open(&local, &remote, 8);

    println!("Connected!")
}
