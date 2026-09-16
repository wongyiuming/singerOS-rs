# const-audio-layout lab

This lab follows the current `min_generic_const_args` direction rather than the older `generic_const_exprs` design.

The important experiment is that an audio block's storage length is derived from `FRAMES × CHANNELS` in the type itself. The current compiler path uses `core::direct_const_arg!` because that is the syntax accepted by moving nightly today.

The syntax is deliberately isolated from production. If rustc changes the generic-const MVP again, only this lab changes.
