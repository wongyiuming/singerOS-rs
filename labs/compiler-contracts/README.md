# compiler-contracts lab

This crate intentionally uses rustc internals that are not production promises.

Current experiment: prove at compile time that a recording bus has an explicit negative contract for program audio.

Run with the pinned nightly:

```bash
RUSTFLAGS="-Zinternal-testing-features -Zpolonius=next" cargo test -p compiler-contracts
```

`negative_bounds` is compiler-internal and may change or disappear. The lab lets SingerOS learn from that direction without making production crates depend on it.
