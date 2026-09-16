## Business change
Explain the user-visible behavior in plain language.

## Data flow
Describe before/after flow, especially browser ↔ server and audio routing.

## Audio graph
State whether the graph changed. If yes, draw it in text and explicitly say whether song/program audio can reach the recording bus.

## API / data format
State `none` or list every changed route/schema/file format.

## Removed behavior
State `none` or list each removal. Silent feature removal is not allowed.

## Experimental Rust
List nightly features, `-Z` flags, unsafe/FFI, compiler-internal features, or new WASM proposals used. State the fallback path.

## Review translation
Explain the hardest Rust code as if the reviewer knows Go/C and basic Rust but not advanced trait/lifetime machinery.

## RN impact
State artifact size, expected idle/runtime CPU impact, and whether deployment requires anything besides replacing compiled artifacts.
