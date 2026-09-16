# Forward research tracks

## Type-level audio safety

Use typestate now and evaluate negative impls/bounds so `MicOnly` and `ProgramMix` are different types rather than flags. Recording APIs should accept only routes proven not to contain program audio.

## AudioWorklet Rust/WASM

Move realtime DSP off the UI thread into an AudioWorklet-backed Rust/WASM engine. CPAL's experimental `audioworklet` backend is compiled in `labs/audio-worklet`; microphone capture remains our own Rust browser capability until web input support is adequate.

## Portable SIMD DSP

`labs/portable-simd-dsp` exercises nightly `portable_simd` for fixed audio blocks. Scalar reference behavior remains the oracle.

## Compiler-next contracts

`labs/compiler-contracts` runs internal negative bounds under Polonius-next. `labs/const-audio-layout` follows the newer generic-const-args machinery so frame/channel topology can migrate into types if the design matures.

## Browser Component Model

Maintain a WIT description of browser capabilities alongside production `web-sys` bindings. When browser-native Component Model/WebIDL integration becomes viable, add a backend rather than rewriting application logic.

## Micro-server edge path

Rust owns TLS and HTTP directly. Benchmark ordinary async file serving first, then experiment with Linux-specific `sendfile`, `splice` or `io_uring` paths only when measurements justify them.
