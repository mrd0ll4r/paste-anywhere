extern crate x11_clipboard;

use std::sync::mpsc::{ Sender, channel , Receiver};
use std::error::Error;
use self::x11_clipboard::Clipboard as X11CB;

pub struct Clipboard{
    x11_clipboard : X11CB,
}

impl Clipboard{
    pub fn init() -> Result<(Self,Receiver<String>),Box<Error>>{
        let (sendr,recvr) = channel();
        let cb=X11CB::new()?;

        Ok((Clipboard{x11_clipboard: cb},recvr))
    }

    pub fn set_contents(&mut self, data: String) -> Result<(), Box<Error>> {
        Ok(self.x11_clipboard.store(
            self.x11_clipboard.setter.atoms.clipboard,
            self.x11_clipboard.setter.atoms.utf8_string,
            data,
        )?)
    }

    pub fn waitForString(&mut self) -> Result<String, Box<Error>>{
        Ok(String::from_utf8(self.x11_clipboard.load(
            self.x11_clipboard.getter.atoms.clipboard,
            self.x11_clipboard.getter.atoms.utf8_string,
            self.x11_clipboard.getter.atoms.property,
            None,
        )?)?)
    }
}


#[test]
fn basics(){
    let (mut cb,recv) = Clipboard::init().unwrap();

    println!("!-----------------------!");
    let stuff=cb.waitForString().unwrap();
    format!("{}\n",stuff);
    println!("!-----------------------!");

    let data="HAHUHY";
    cb.set_contents(data.to_string());
    assert!(cb.waitForString().unwrap().eq(data));
    assert!(cb.waitForString().unwrap().eq(data));

//    let data = format!("{:?}", Instant::now());
//    let clipboard = Clipboard::new().unwrap();
//
//    let atom_clipboard = clipboard.setter.atoms.clipboard;
//    let atom_utf8string = clipboard.setter.atoms.utf8_string;
//    let atom_property = clipboard.setter.atoms.property;
//
//    clipboard.store(atom_clipboard, atom_utf8string, data.as_bytes()).unwrap();
//
//    let output = clipboard.load(atom_clipboard, atom_utf8string, atom_property, None).unwrap();
//    assert_eq!(output, data.as_bytes());
//
//    let data = format!("{:?}", Instant::now());
//    clipboard.store(atom_clipboard, atom_utf8string, data.as_bytes()).unwrap();
//
//    let output = clipboard.load(atom_clipboard, atom_utf8string, atom_property, None).unwrap();
//    assert_eq!(output, data.as_bytes());
//
//    let output = clipboard.load(atom_clipboard, atom_utf8string, atom_property, None).unwrap();
//    assert_eq!(output, data.as_bytes());
}
