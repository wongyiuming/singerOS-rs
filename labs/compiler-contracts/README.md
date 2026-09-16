# compiler-contracts lab

This crate intentionally uses rustc internals that are not production promises.

Current experiment: prove at compile time that a recording bus has an explicit negative contract for program audio.

Run on moving nightly:

```bash
RUSTFLAGS="-Zinternal-testing-features -Zpolonius=next" \
  cargo +nightly test --manifest-path labs/compiler-contracts/Cargo.toml --lib
```

`negative_bounds` is compiler-internal and may change or disappear. This lab is deliberately outside the production workspace so a parser/feature break cannot block SingerOS mainline.
