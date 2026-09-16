# Rust PR review guide for SingerOS

You do not need to understand every advanced Rust mechanism to review SingerOS safely. Review in layers.

## Pass 1: business behavior

Read public types, route names, state enums and tests first. Confirm what changed for users, which existing behavior remains, and whether anything was removed.

## Pass 2: data and audio flow

For browser/audio changes, trace sources and sinks rather than syntax. A recording PR must make the graph explicit. Song/program audio reaching a mic-only recording route is a correctness failure regardless of how elegant the Rust looks.

## Pass 3: ownership and concurrency

Look for who owns each long-lived resource: microphone stream, AudioContext, recorder, file handle, catalog snapshot and server state. Pay special attention to `Arc`, `Rc`, `Mutex`, `RwLock`, channels and spawned tasks.

## Pass 4: advanced-Rust danger markers

Require an explanation for `unsafe`, raw pointers, `transmute`, `Pin`, `ManuallyDrop`, manual `Send`/`Sync`, FFI, custom proc-macros, specialization, internal rustc features, `-Z` flags and non-obvious lifetime machinery.

Nightly itself is not a reason to reject a PR. Unexplained nightly is.

## Hot-path questions

On RN/server code: does this add parsing, allocation, hashing, locking, polling or background work to a request path?

On WASM/audio code: does this allocate or lock in a realtime callback? Does it move work back onto the UI thread? Does it require a browser capability Tesla may not provide?

## PR rule

The PR template's `Review translation` section is mandatory for advanced code: author explains the hard Rust in Go/C/basic-Rust terms and states the invariant the compiler is being asked to enforce.
