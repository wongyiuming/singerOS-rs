# SingerOS-rs

All-Rust successor to SingerOS.

Goals:
- zero handwritten business JavaScript;
- Rust/WASM browser application owns UI, microphone, WebAudio, recording, lyrics and state;
- Rust server owns the production HTTP API, recording persistence and karaoke catalog/assets;
- Rust server can directly terminate TLS;
- shared Rust types define browser/server protocol;
- runtime design budget is roughly 0.15 normal vCPU and may shrink further;
- production-capable crates and deliberately unstable compiler/WebAssembly labs coexist.

Nightly Rust is pinned. Experimental rustc internals live under `labs/` until they prove useful enough to promote.

## Production API parity

The Rust server keeps the existing `/singeros` production contract during migration, including health/capabilities, client logs, chunked recordings, karaoke catalog/assets and karaoke recordings. Existing recording and karaoke data remains under `/opt/singeros` on RN.

## Runtime policy

RN is a deployment-only node. It receives CI-built release artifacts; it does not contain Rust toolchains, source checkouts, build caches or development dependencies.

SingerOS-rs has no SingerOS-specific Redis, SQL server or service mesh. The final target is native Rust TLS/HTTP without the legacy Go runtime; compatibility infrastructure is retained only for the controlled cutover and removed after public verification.
