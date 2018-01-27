# paste-anywhere

[![Build Status](https://api.travis-ci.org/mrd0ll4r/paste-anywhere.svg?branch=master)](https://travis-ci.org/mrd0ll4r/paste-anywhere)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](https://opensource.org/licenses/MIT)

Synchronize your clipboard

## Building
You need a Rust __nightly__ toolchain for this to work.
Use [rustup.sh](https://rustup.rs/) to get Rust, then set your default toolchain:

```sh
rustup default nightly
rustup update
```

Then you should be able to build the project using
```sh
cargo build
```

Optionally, use
```sh
cargo test
```
to run unit-tests.

## Exploring the source
The source files are roughly responsible for modules of the project like so:

- `network.rs` handles low-level networking:
    Establishing connections, reading, writing, (de)serialization, ...
- `overlay.rs` builds a Gnutella-like overlay on top of that.
- `clock.rs` implements a vector clock.
- `main.rs` is the entry point for the application.