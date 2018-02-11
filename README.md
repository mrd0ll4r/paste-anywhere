# paste-anywhere

[![Build Status](https://api.travis-ci.org/mrd0ll4r/paste-anywhere.svg?branch=master)](https://travis-ci.org/mrd0ll4r/paste-anywhere)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://opensource.org/licenses/MIT)

Synchronize your clipboard

## Dependencies

for building and running, you'll need:
- Linux with X11 (compatibility with wayland not tested)
- rustup (to set up the toolchain, described below)

## Building
You need a Rust __nightly__ toolchain for this to work.
Use [rustup.rs](https://rustup.rs/) to get Rust, then set your default toolchain:

```sh
rustup default nightly
rustup update
```

Then you should be able to build the project using
```sh
cargo build
```
or, for building a release binary (optimized and without debug options):
```sh
cargo build --release
```

Optionally, use
```sh
cargo test
```
to run unit-tests.

## Running
execute 
```sh
./build/debug/paste-anywhere <local-ip> [<bootstrap-peer>...]
```
or, for release builds:
```sh
./build/release/paste-anywhere <local-ip> [<bootstrap-peer>...]
```

 - `<local-ip>` should be the ipv4-adress this program runs on.
 - `<bootstrap-peer>` is the `ipv4:port` address 
 of a known running peer, to bootstrap the overlay network.
 the port is printed out on the commandline output of the program.

## Exploring the source
The source files are roughly responsible for modules of the project like so:

- `network.rs` handles low-level networking:
    Establishing connections, reading, writing, (de)serialization, ...
- `overlay.rs` builds a Gnutella-like overlay on top of that.
- `clock.rs` implements a vector clock.
- `main.rs` is the entry point for the application.
