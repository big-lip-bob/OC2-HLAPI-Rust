# OpenComputers II HLAPI Rust Wrapper
A simple yet efficient wrapper around the HLAPI `/dev/hvc0` stty port in OpenComputers II written in Rust for Rust

---
##### TODO
- Remove / find an alternative to `serde` due to its very fat size (80kB)
- Eventually add a `build.rs` that would generate component and their associated Rust traits from a JSON dump
- Make it all `#![no_std]` ? (doubt i got enough sanity)
