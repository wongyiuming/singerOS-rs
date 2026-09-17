# AudioWorklet lab

This lab verifies that the pinned nightly can compile CPAL's WebAssembly AudioWorklet backend with Wasm atomics and a rebuilt standard library.

It does **not** assume CPAL owns microphone capture. SingerOS microphone permission and `getUserMedia()` remain behind our Rust `web-sys` browser capability layer unless CPAL gains a suitable async browser-input API.

Required build shape:

```bash
RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals" \
  cargo check -p audio-worklet-lab \
  --target wasm32-unknown-unknown \
  -Zbuild-std=std,panic_abort
```

Runtime use also requires cross-origin isolation so `SharedArrayBuffer` is available.
