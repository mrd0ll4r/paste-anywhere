#![feature(mpsc_select)]

extern crate byteorder;
extern crate rand;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

mod clock;
mod network;
mod overlay;
mod clipboard;

use clock::*;
use network::*;
use overlay::*;

use std::net::*;
use std::env;
use std::thread;
use std::time;
use clipboard::Clipboard;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() == 1 {
        let o = Overlay::new(&Ipv4Addr::new(127, 0, 0, 1), Vec::new()).unwrap();

        o.start_accepting();
        o.start_autoping();

        println!("performing join...");
        let join = o.perform_join();
        if let Err(e) = join {
            println!("join failed: {}", e);
        }

        for i in 0..100 {
            thread::sleep(time::Duration::new(17, i * 1000 * 1000 * 10));
            println!("setting clipboard...");
            o.set_clipboard(&"first".to_string());
        }

        println!("going to sleep...");
        thread::sleep(time::Duration::new(60 * 60, 0));
    }

    if args[1].starts_with("clip") {
        let mut cbi = Clipboard::init().unwrap();
        println!("!-----------------------!");
        let mut cb = cbi.0;
        print!("{:?}\n", cb.waitForString().unwrap());
        print!("{:?}\n", "HI");
        cb.set_contents("DASDA".to_string());
        print!("{:?}\n", cb.waitForString().unwrap());
        thread::sleep(time::Duration::from_secs(10));
        println!("!-----------------------!");
        let x = cb.waitForString().unwrap();
        print!("{:?}\n", x);
        return;
    }

    let mut bootstrap_peers: Vec<Endpoint> = Vec::new();
    for i in 1..args.len() {
        let p = &args[i];
        let addr: SocketAddr = p.parse().unwrap();
        if !addr.is_ipv4() {
            panic!("address is not IPv4")
        }
        if let SocketAddr::V4(v4) = addr {
            bootstrap_peers.push(Endpoint::new(v4.ip(), v4.port()));
        }
    }

    let o = Overlay::new(&Ipv4Addr::new(127, 0, 0, 1), bootstrap_peers).unwrap();

    o.start_accepting();
    o.start_autoping();

    println!("performing join...");
    let join = o.perform_join();
    if let Err(e) = join {
        println!("join failed: {}", e);
    }

    for i in 0..100 {
        thread::sleep(time::Duration::new(19, i * 1000 * 1000 * 10));
        println!("setting clipboard...");
        o.set_clipboard(&"not first".to_string());
    }

    println!("going to sleep...");
    thread::sleep(time::Duration::new(60 * 60, 0));
}
