# livox-rs

[![Livox on Travis CI][travis-image]][travis]

[travis-image]: https://api.travis-ci.com/lkolbly/livox-rs.svg?branch=master
[travis]: https://travis-ci.com/lkolbly/livox-rs

A Rust library for streaming data from Livox LiDAR devices

To get started, connect your computer to a network with a Livox device on it (I tested with a single MID-40), and run the example:
```
cargo run --example dump
```
It should tell the LiDAR to power on, and stream data to a `points.laz` file.
