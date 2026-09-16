# const-route-kind lab

This lab tests `min_adt_const_params` as a future replacement or complement for marker-type typestate.

Instead of `AudioGraph<MicOnly>`, the route itself is a const enum value:

```rust
AudioGraph<{ RouteKind::MicOnly }>
```

Only the `MicOnly` specialization exposes `attach_recorder()`. `ProgramMix` has no recording API at all.

If this remains robust, SingerOS can eventually encode route, channel layout or processing mode as const values while keeping illegal audio topology unrepresentable.
