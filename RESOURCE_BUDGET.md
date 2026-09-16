# Resource budget

Production target is an old Intel Xeon E5-class RN node whose measured effective compute is roughly 0.15 of a normal vCPU. The provider's displayed vCPU count is not used as the engineering budget.

Rules:
- one SingerOS server process;
- Tokio current-thread runtime unless measurements prove otherwise;
- no build toolchain or source checkout on RN;
- no polling loop where event-driven state works;
- no server-side routine audio decode, separation, transcoding, DSP or SSR;
- immutable/in-memory read state where practical;
- sequential append and atomic snapshots instead of general database machinery until query complexity actually requires more;
- precompute expensive indexes at mutation time;
- browser/WASM owns realtime audio, DSP, UI and lyric timing.

Native release target is `sandybridge`: AVX/AES/SSE4.2 are available; AVX2 is not assumed. CI must never build RN production binaries with `target-cpu=native`.

CPU is scarcer than memory. Trading bounded memory for fewer parses, hashes, directory scans, allocations or synchronization operations is normally preferred.
