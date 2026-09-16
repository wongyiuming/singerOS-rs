# Forward research tracks

## Type-level audio safety

Use typestate now and evaluate negative impls/bounds so `MicOnly` and `ProgramMix` are different types rather than flags. Recording APIs should accept only routes proven not to contain program audio.

## AudioWorklet Rust/WASM

Move realtime DSP off the UI thread into an AudioWorklet-backed Rust/WASM engine. Evaluate Wasm atomics and SharedArrayBuffer under strict COOP/COEP headers. Targets include mixer, monitor gain, limiter, EQ, feedback suppression, meter and later pitch/effects.

## Portable SIMD DSP

Evaluate nightly `portable_simd` for blocks of `f32`. Keep scalar reference implementations and differential tests.

## Compiler-next contracts

Exercise the next trait solver and Polonius-next. Track new generic-const-args work as a future way to encode channels, sample rate, block size and graph shape in types.

## Browser Component Model

Maintain a WIT description of browser capabilities alongside production `web-sys` bindings. When browser-native Component Model/WebIDL integration becomes viable, add a backend rather than rewriting application logic.

## Micro-server edge path

Rust owns TLS and HTTP directly. Benchmark ordinary async file serving first, then experiment with Linux-specific `sendfile`, `splice` or `io_uring` paths only when measurements justify them.
