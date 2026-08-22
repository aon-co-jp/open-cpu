> Japanese original / 日本語原文: [CLAUDE.md](../CLAUDE.md)

# Development Policy & Development Environment Rules (open-cpu)

This repository is a member of the `aon-co-jp` ecosystem. **The design philosophy
and operational rules common to all repositories (thorough verification, no
exaggerated reporting, avoiding reinventing the wheel, continuing automatically
without asking for confirmation, etc.) are governed by
[`open-raid-z/CLAUDE.md`](https://github.com/aon-co-jp/open-raid-z/blob/main/CLAUDE.md)
as the authoritative source.** They are not duplicated here; only matters
specific to this repository are described.

The working drive is `F:\runo\open-cpu` (the new layout).

## Role of this repository

`open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` were each **about to
implement their own** runtime detection and dispatch for CPU instruction sets
(AVX2/AVX-512/PCLMULQDQ/BMI1/BMI2/FMA3/AES-NI/POPCNT/SHA-NI), so this repository
was created as a shared foundation for that.

- **It is a library crate** (user instruction, 2026-08-22). It must not be turned
  into a resident service (daemon). Each repository adds it to `[dependencies]`
  and links it into the same process. Do not introduce inter-process
  communication.
- The scope is limited to "CPU feature detection" and "dispatching computation
  based on that detection result." Do not port the domain logic of each
  repository (RAID striping strategy, DB page management, LLM model loading,
  etc.) into here.
- Do not add dependency crates (currently `[dependencies]` is empty, `std` only).
  Because this is a foundation that sits at the very bottom of each repository,
  bringing in a dependency would ripple through everything.

## Implementation and verification rules (specific to this repository)

- **Whenever you add a SIMD implementation, you must write an output-match test
  against the scalar implementation at the same time.** Be sure to include
  remainders (lengths that are not a multiple of the vector width) in the test
  cases.
- **Code paths that cannot be executed on real hardware must be clearly marked
  "unverified" and must not be selected by the default dispatch.** Currently the
  AVX-512 path falls into this category (the development machine's AMD Ryzen 9
  3950X does not have AVX-512). It becomes enabled only as an opt-in via
  `OPEN_CPU_ENABLE_AVX512=1`. Once verification on an AVX-512-equipped machine is
  complete, remove this restriction and update the "unverified" wording in
  README.md / PORTING.md / this file.
- When writing benchmark numbers, only write **what was actually measured** as
  "measured." Do not write estimated or theoretical values as if they were
  measured.
- Do not use `cargo bench`; the simple measurement in `examples/bench.rs` using
  `std::time::Instant` is sufficient (so as not to add dependencies).

## Multilingual documentation

The `README/` folder contains README / CLAUDE / PORTING in 15 languages
(an ecosystem-wide practice, using the same naming convention as `open-raid-z` /
`open-cuda`). **The Japanese versions (`README.md` / `CLAUDE.md` / `PORTING.md`
at the repository root) are authoritative**, and when their content is updated
the 15 language versions must be brought in line as well (there is no automatic
synchronization mechanism; reflect the changes manually).

Languages: US English / UK English / Germany / Italy / France / Spain / Russia /
Ukraine / Hebrew / Persian (Iran) / Arabic / China / Taiwan / Korea / Japan.

## HANDOFF

- **2026-08-22 newly created + integrated into 2 repositories**:
  The initial implementation was created starting from an empty repository.
  - `src/caps.rs`: the `CpuCapabilities` struct and `detect()` (`OnceLock`
    cached). The detection targets are avx2 / avx512f / avx512bw / avx512vl /
    pclmulqdq / bmi1 / bmi2 / fma / aes / popcnt / sha / sse2 / ssse3 / avx-vnni /
    avx512vnni.
  - `src/gf.rs`: RAID6 GF(2^8) operations (polynomial 0x11d, generator g=2).
    `gf_xor` / `gf_mul_parity` / `raid6_parity` / `raid6_coeff` are public, and
    the four implementations scalar / PCLMULQDQ / AVX2 / AVX-512 are dispatched
    at run time.
  - Measured (Ryzen 9 3950X, 4MiB×50 times): scalar 1003 MiB/s, pclmulqdq
    1916 MiB/s (1.91x), **avx2 22531 MiB/s (22.46x)**. AVX-512 could not be
    measured since it is not present.
    → **It later turned out that this avx2 speedup varies within the range of
    11.6–18.1x on re-measurement** (because the SIMD side is limited by memory
    bandwidth. The "Measured benchmarks" section of README.md is authoritative
    for the latest numbers and ranges).
  - `cargo test --release` passed all 11 tests + 1 doctest. The PCLMULQDQ
    implementation was confirmed to match the scalar one for all 256
    coefficients.
  - Integration record: incorporated via path dependencies into `open-raid-z`
    (replacement of GF operations) and `open-english/server` (the
    `/v1/cpu-runtime` endpoint and the startup log).

- **2026-08-22 (continued) practicality improvement cycle + self-introduction
  feature**:
  After the initial implementation, a develop → TEST → fix cycle was carried out
  to improve interoperability and practicality.
  - **Cycle 1: adding the Horner's method APIs** (`gf_mul_pow2_xor` /
    `gf_mul2_xor` / `gf_mul4_xor`, AVX2 + scalar). This made it possible to
    delegate the AVX2 paths of `open-raid-z`'s `mul2_xor_into` /
    `mul4_xor_into` to open-cpu as well, further reducing the x86 code on that
    side. An AVX2 path was also added to `gf_xor` (so that the delegation would
    not become a performance regression).
  - **Cycle 2: usability improvements**. A `Display` implementation and
    `has_all()` were added to `CpuCapabilities`. `raid6_parity3()`, which
    computes the RAID-Z3-equivalent P/Q/R in bulk, was added. Measurements of
    XOR and Horner's method were added to `examples/bench.rs`.
  - **Verification**: `cargo test --release` **passed all 15 tests + 2
    doctests**. The benchmark was run four times in a row, and the variance
    (varying between 11.6–18.1x because the SIMD side is limited by memory
    bandwidth) was honestly recorded in the README as a range.
  - **A real bug found while integrating into `open-english` (an important
    lesson)**: the self-introduction response feature for "who made you" was
    implemented using a keyword substring match (`"誰が作"`), and **testing in a
    real browser revealed that "誰が【このシステムを】作ったのですか?" could not
    be detected** (because words come between the interrogative and the verb).
    It was fixed into a form that separates the interrogative list and the verb
    list and evaluates them with an AND condition, and 10 positive and 6 negative
    examples were checked along with an E2E check in a real browser. This is a
    record that it was **the kind of bug that cannot be found unless you go as
    far as "actually typing into the chat," rather than stopping at "the unit
    tests passed."**

- **What to do next**:
  1. Progressively integrate it into the checksum/compression areas of
     `aruaru-db`, the matrix operations of `aruaru-llm`, and the CPU fallback of
     `open-cuda` (currently only the 2 cases `open-raid-z` and `open-english`).
  2. Once an AVX-512-equipped machine can be obtained or secured, verify the
     AVX-512 path by actual measurement and remove the opt-in restriction.
  3. For the instruction sets that are detection-only with no implementation
     (POPCNT/BMI/FMA/AES-NI/SHA-NI), add computational implementations starting
     with the ones actually needed on the dependent side. In particular, CRC32C
     via PCLMULQDQ is a candidate for `aruaru-db`'s checksums, and int8 dot
     products via AVX-VNNI/AVX-512 VNNI for `aruaru-llm`.
  4. Since it is wired up with a path dependency (`path = "../open-cpu"`),
     building in an environment without the `F:\runo` layout, such as a VPS, will
     require switching to a git dependency. Handle this when it actually comes
     time to build on the VPS.
  5. Because there is no implementation for SSE2-only CPUs, `open-raid-z` still
     keeps its own implementation for the SSE2 path only. Adding an SSE2 version
     would allow that to be delegated as well.
  6. Even when SIMD-ized, `gf_xor` only reaches 1.12–1.25x versus scalar
     (limited by memory bandwidth). An optimization using non-temporal stores
     (`_mm256_stream_si256`) to avoid cache pollution may be effective, and is
     worth trying for large buffers (unverified).
