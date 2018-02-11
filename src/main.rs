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

use std::sync::Arc;
use std::net::*;
use std::env;
use std::thread;
use std::time;
use clipboard::Clipboard;
use std::sync::Mutex;
use rand::Rng;

fn main() {
    let args: Vec<String> = env::args().collect();
    let (cb,_)=Clipboard::init().unwrap();
    let c= Arc::new(Mutex::new(cb));

//    if args[1].starts_with("clip") {
//        let mut cbi = Clipboard::init().unwrap();
//        println!("!-----------------------!");
//        let mut cb = cbi.0;
//        cb.set_contents("DASDA34".to_string());
//        print!("{:?}\n", cb.get_contents().unwrap());
//        print!("{:?}\n", "HI");
//        cb.set_contents("DASDA".to_string());
//        print!("{:?}\n", cb.get_contents().unwrap());
//        thread::sleep(time::Duration::from_secs(10));
//        println!("!-----------------------!");
//        let x = cb.get_contents().unwrap();
//        print!("{:?}\n", x);
//        return;
//    }

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

    let o = Arc::new(Overlay::new(&Ipv4Addr::new(127, 0, 0, 1), bootstrap_peers).unwrap());

    o.start_accepting();
    o.start_autoping();

    println!("performing join...");
    let join = o.perform_join();
    if let Err(e) = join {
        println!("join failed: {}", e);
    }

    {
        let oo = o.clone();
        let cc = c.clone();
        thread::spawn(move || {
            for i in 0..100 {
                thread::sleep(time::Duration::new(17, i * 1000 * 1000 * 10));
                println!("getting clipboard...");
                let resp = oo.get_clipboard();
                match resp {
                    Err(e) => println!("unable to get clipboard: {}", e),
                    Ok(Some(content)) => {
                        println!("clipboard is: {}", content.clone());
                        cc.lock().unwrap().set_contents(content);
                    },
                    Ok(None) => println!("clipboard is local!"),
                };
            }
            println!("-oo-supervisor-thread-closed-");
        });
    }
    let mut rng=rand::thread_rng();
    while true{
        let cb_content=c.lock().unwrap().get_contents();
        match cb_content{
            Err(e) => println!("\tunable to get local clipboard: {}", e),
            Ok(Some(content)) => {
                println!("\tlocal clipboard is: {}", content.clone());
                o.set_clipboard(content.clone().as_ref());
            },
            Ok(None) => println!("\tclipboard has not changed!"),
        }
        //sleep 200ms + random?
//        let ns=rng.gen_range();
        thread::sleep(time::Duration::new(0,200*1000*1000))
    }


//    //TESTING
//    for i in 0..100 {
//        if args.len()==1 {
//            thread::sleep(time::Duration::new(23, i * 1000 * 1000 * 10));
//            println!("setting clipboard...");
//            o.set_clipboard("first");
//        }else {
//            thread::sleep(time::Duration::new(19, i * 1000 * 1000 * 10));
//            println!("setting clipboard...");
//            o.set_clipboard("not first");
//        }
//    }
//
//    println!("going to sleep...");
//    thread::sleep(time::Duration::new(60 * 60, 0));
}
