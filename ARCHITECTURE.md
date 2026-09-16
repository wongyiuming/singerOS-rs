# Architecture

## Prime directive

SingerOS-rs is an all-Rust system. Handwritten business JavaScript is not accepted. Generated wasm-bindgen glue is a compatibility artifact, not application source.

## Browser plane

`crates/singer-web` is a Leptos CSR application compiled to WebAssembly. It owns UI, state machines, lyrics timing, microphone permissions, recording control and user interaction.

`crates/singer-audio` owns the browser audio graph. UI components do not manipulate WebAudio nodes directly. Recording topology must make it structurally difficult to route song/program audio into a mic-only recording.

## Server plane

`crates/singer-server` directly terminates TLS with rustls and serves HTTP with Axum/Hyper. There is no SingerOS Nginx dependency. Tokio uses the current-thread runtime because the deployment target has about 0.15 normal-vCPU effective compute.

RN is runtime-only: CI builds artifacts; RN never builds Rust.

## Compiler policy

The workspace deliberately pins nightly. Production crates may use nightly capabilities when they remove measurable runtime cost or enforce a useful invariant. Compiler-internal experiments live under `labs/` and are never silently promoted.

## Migration rule

The existing Go SingerOS remains the production reference until Rust reaches behavioral parity. Existing URLs, catalog semantics, recording semantics and browser behavior must be captured by tests before cutover. No feature is removed merely to simplify the Rust port.

## Runtime dependencies

No SingerOS-specific Nginx, Redis, SQL server, Kafka/NATS, sidecar or service mesh is part of the target runtime. SingerOS implements only the narrow capabilities it actually needs: typed local persistence, bounded in-process state, TLS/HTTP, Range serving, rate limiting and structured append-only telemetry.
