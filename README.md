# OpenComputers II HLAPI Rust Wrapper
A simple yet efficient wrapper around the HLAPI `/dev/hvc0` stty port in OpenComputers II written in Rust for Rust

---
### TODO
- Remove / find an alternative to `serde` due to its very fat size (80kB), or use dynamic dispatching, or alternatively make `serde` a dynamic library
- Eventually add a `build.rs` that would generate component and their associated Rust traits from a JSON dump (delegated to `OC2Generator`)
- ~~Make it all `#![no_std]` ? (doubt i got enough sanity)~~ (`/dev/hvc0` being a feature of the Linux image, `std` will be available (a MMIO variant of this for baremetal ?))
