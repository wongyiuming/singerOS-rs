# SingerOS-rs

Experimental all-Rust successor to SingerOS.

Goals:
- zero handwritten business JavaScript;
- Rust/WASM browser application owns UI, microphone, WebAudio, recording, lyrics and state;
- Rust server directly terminates TLS and serves HTTP;
- shared Rust types define browser/server protocol;
- runtime design budget is roughly 0.15 normal vCPU and may shrink further;
- production-capable crates and deliberately unstable compiler/WebAssembly labs coexist.

This repository is intentionally aggressive. Nightly Rust is pinned. Experimental rustc internals live under `labs/` until they prove useful enough to promote.

## Runtime policy

RN is a deployment-only node. It receives CI-built release artifacts; it does not contain Rust toolchains, source checkouts, build caches or development dependencies.

SingerOS-rs has no SingerOS-specific Nginx, Redis, SQL server, sidecar or service mesh in the target architecture.
