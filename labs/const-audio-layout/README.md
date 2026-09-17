# const-audio-layout lab

This lab tracks rustc's current `generic_const_args` direction rather than the older `generic_const_exprs` design.

On the 2026-09-16 moving nightly, the public-facing `min_generic_const_args` gate alone does not make the parser accept `type const`. rustc still exposes the transitional `mgca_type_const_syntax` gate for enabling that syntax before expansion, so the lab explicitly tests that compiler-internal transition layer too.

The experiment derives an `AudioBlock` array length from `FRAMES × CHANNELS` as a generic type-level constant. The crate is outside production workspace; parser/feature churn can never block SingerOS mainline.
