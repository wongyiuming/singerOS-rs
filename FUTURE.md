# Forward research tracks

## Type-level audio safety

Use typestate now and evaluate negative impls/bounds so `MicOnly` and `ProgramMix` are different types rather than flags. `labs/const-route-kind` goes further by encoding the route as an ADT const parameter, so only the mic-only specialization exposes a recording API.

## AudioWorklet Rust/WASM

Move realtime DSP off the UI thread into an AudioWorklet-backed Rust/WASM engine. CPAL's `audioworklet` backend is compile-verified in `labs/audio-worklet` with Wasm atomics and a rebuilt standard library. Microphone capture remains our own Rust browser capability until web input support is adequate.

## Portable SIMD DSP

`labs/portable-simd-dsp` exercises nightly `portable_simd` for fixed audio blocks. Scalar behavior remains the oracle.

## Compiler-next contracts

`labs/compiler-contracts` verifies internal negative bounds under Polonius-next. `labs/const-audio-layout` tracks the MGCA/type-const parser transition as a probe: current nightly advertises the feature metadata while its exact parser build rejects the syntax, so this is explicitly recorded as upstream-blocked rather than worked around in production.

## Browser Component Model

Maintain a WIT description of browser capabilities alongside production `web-sys` bindings. When browser-native Component Model/WebIDL integration becomes viable, add a backend rather than rewriting application logic.

## Micro-server edge path

Rust owns TLS and HTTP directly. Benchmark ordinary async file serving first, then experiment with Linux-specific `sendfile`, `splice` or `io_uring` paths only when measurements justify them.
