# const-audio-layout lab

This lab follows the current `min_generic_const_args` / `generic_const_args` direction rather than the older `generic_const_exprs` design.

The target language spelling discussed by rustc is `type const`, but the 2026-09-16 moving nightly parser does not yet accept that spelling. The compiler's current transition mechanism is `#[type_const] const`, so this lab intentionally uses the implementation that exists today rather than pretending future syntax is already available.

The experiment makes an audio block's storage length derive from `FRAMES × CHANNELS` in the type itself. When rustc flips to the final syntax, only this isolated lab should need a syntax migration.
