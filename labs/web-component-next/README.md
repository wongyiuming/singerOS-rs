# web-component-next lab

Goal: model SingerOS browser capabilities with WIT now, so a future browser-native WebAssembly Component Model/WebIDL binding can replace today's generated JS glue without rewriting the Rust application.

2026 deployment reality: `wasm-bindgen`/`web-sys` remains the production bridge for microphone, WebAudio and MediaRecorder. This lab is a parallel contract, not a claim that native browser Component Model bindings already replace that bridge.
