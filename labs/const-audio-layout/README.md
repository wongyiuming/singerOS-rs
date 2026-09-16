# const-audio-layout lab

This lab tracks rustc's current `generic_const_args` direction rather than the older `generic_const_exprs` design.

It deliberately mirrors the current Unstable Book shape: a generic `type const` computes `FRAMES × CHANNELS`, and that named type-level constant becomes the array length of `AudioBlock`.

This crate is outside the production workspace. If moving nightly and the Unstable Book disagree during an in-flight syntax transition, that disagreement is treated as an experiment result, never as a production blocker.
