> Japanese original / 日本語原文: [README.md](../README.md)

# open-cpu

A **CPU instruction set detection and runtime dispatch library** (Rust) shared
across the `aon-co-jp` ecosystem.

It was created to avoid `open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda`
each writing their own duplicate CPU feature detection code.

**It is not a resident service (daemon).** It is an ordinary library crate that
each repository adds to the `[dependencies]` section of its `Cargo.toml` and
links into the same process.

## What it can do

1. **Runtime CPU feature detection** — `open_cpu::detect()` returns
   `&'static CpuCapabilities`. It uses `std::is_x86_feature_detected!` and
   caches the first detection result in a `OnceLock`, so the cost of calling it
   repeatedly is close to zero.
2. **Runtime dispatch of RAID6 GF(2^8) operations** — depending on the detection
   result, it selects the scalar / PCLMULQDQ / AVX2 / AVX-512 implementation at
   run time.

## Usage

```toml
# Cargo.toml on the dependent side
[dependencies]
open-cpu = { path = "../open-cpu" }
```

```rust
// 1. CPU feature detection
let caps = open_cpu::detect();
println!("{}", caps);   // Display is implemented
// => sse2 ssse3 popcnt aes pclmulqdq bmi1 bmi2 fma sha avx2

if caps.avx2 { /* ... */ }
if caps.has_all(&[caps.avx2, caps.fma]) { /* AVX2+FMA3 path */ }

// 2. RAID6 parity (coefficient table method)
let d0 = vec![1u8; 4096];
let d1 = vec![2u8; 4096];
let mut p = vec![0u8; 4096];
let mut q = vec![0u8; 4096];
open_cpu::raid6_parity(&[&d0, &d1], &mut p, &mut q);

// 3. RAID-Z3 equivalent P/Q/R (Horner's method, fast, no coefficient table needed)
let mut r = vec![0u8; 4096];
open_cpu::raid6_parity3(&[&d0, &d1], &mut p, &mut q, &mut r);

// Individual APIs
open_cpu::gf_xor(&mut p, &d0);                 // p ^= d0          (P parity)
open_cpu::gf_mul_parity(&mut q, &d0, 0x02);    // q ^= d0 * 0x02   (Q parity)
open_cpu::gf_mul2_xor(&mut q, &d0);            // q = q*2 ^ d0     (Horner's method)
open_cpu::gf_mul4_xor(&mut r, &d0);            // r = r*4 ^ d0     (Horner's method)

// One-line log summary
println!("{}", open_cpu::runtime_summary());
// => open-cpu 0.1.0 | features: ... | gf impl: Avx2
```

### List of public APIs

| API | Description | Dispatch |
|---|---|---|
| `detect() -> &'static CpuCapabilities` | CPU feature detection (`OnceLock` cached) | — |
| `runtime_summary() -> String` | One-line summary of the detection result + selected implementation | — |
| `selected_impl() -> GfImpl` | The implementation selected for GF operations | — |
| `gf_xor(dst, src)` | `dst ^= src` (P parity) | AVX2 / scalar |
| `gf_mul_parity(dst, src, factor)` | `dst ^= src * factor` (Q parity) | AVX-512 (opt-in) / AVX2 / PCLMULQDQ / scalar |
| `gf_mul_pow2_xor(acc, src, times)` | `acc = acc * 2^times ^ src` | AVX2 / scalar |
| `gf_mul2_xor` / `gf_mul4_xor` | The above with `times=1` / `times=2` | Same as above |
| `raid6_parity(stripes, p, q)` | Computes P/Q in bulk using the coefficient table method | Follows the above |
| `raid6_parity3(stripes, p, q, r)` | Computes P/Q/R in bulk using Horner's method | Follows the above |
| `gf_mul(a, b) -> u8` | Single-byte GF multiplication (`const fn`) | — |
| `gf_mul2_byte(b) -> u8` | Doubling a single byte over GF (`const fn`) | — |
| `raid6_coeff(i) -> u8` | The RAID6 coefficient `g^i` (`g = 2`) | — |

`*_scalar` / `*_avx2` / `*_pclmul` / `*_avx512` versions that call each
implementation explicitly are also public (for benchmarking and cross-validation;
the SIMD versions are `unsafe`).

## Instruction sets covered by detection

| Instruction set | Detection | Use within this crate |
|---|---|---|
| SSE2 | ✅ | Used as a helper for the PCLMULQDQ path |
| SSSE3 | ✅ | `pshufb` (reduction in the PCLMULQDQ path) |
| PCLMULQDQ | ✅ | GF(2^8) multiplication (carry-less multiplication implementation) |
| AVX2 | ✅ | GF(2^8) multiplication (`vpshufb` split-table, selected by default) |
| AVX-512F / BW / VL | ✅ | A GF(2^8) multiplication path exists (**execution unverified**, see below) |
| POPCNT | ✅ | Detection only (no implementation using it) |
| BMI1 / BMI2 | ✅ | Detection only (no implementation using it) |
| FMA3 | ✅ | Detection only (no implementation using it) |
| AES-NI | ✅ | Detection only (no implementation using it) |
| SHA-NI | ✅ | Detection only (no implementation using it) |
| AVX-VNNI | ✅ | Detection only (for future AI inference, no implementation using it) |
| AVX-512 VNNI | ✅ | Detection only (for future AI inference, no implementation using it) |

On architectures other than x86/x86_64 all fields become `false` and the code
falls back to the scalar implementation (the build still succeeds).

## Details of the GF(2^8) implementation

The irreducible polynomial is `0x11d` (x^8+x^4+x^3+x^2+1) and the generator is
`g = 2`. This is the same as Linux md/RAID6 and ZFS RAID-Z.

- **Scalar**: a reference implementation using nibble split tables
  (16 entries × 2).
- **PCLMULQDQ**: when each byte is expanded to 16-bit intervals, the carry-less
  product with an 8-bit coefficient (at most 15 bits) does not carry over into
  the adjacent slot. Using this property, 4 bytes are multiplied together in a
  single instruction, and reduced to GF(2^8) with two `pshufb` operations.
  16 byte/iter.
- **AVX2**: a split-table implementation using `vpshufb`. 32 byte/iter.
- **AVX-512F/BW**: processes the same split table at 64 byte/iter.

## Measured benchmarks

`cargo run --release --example bench` (4 MiB × 50 times = 200 MiB, factor=0x8d)

Development machine: **AMD Ryzen 9 3950X** / Windows 11 / rustc 1.96.0

```
open-cpu 0.1.0 | features: sse2 ssse3 popcnt aes pclmulqdq bmi1 bmi2 fma sha avx2 | gf impl: Avx2
scalar   :  207.5 ms     963 MiB/s
pclmulqdq:  109.0 ms    1835 MiB/s  (1.90x vs scalar)
avx2     :   11.4 ms   17473 MiB/s  (18.14x vs scalar)
avx512   : not present on this CPU, so it cannot be measured (unverified)

xor scalar     :  11.5 ms   17451 MiB/s
xor dispatch   :   9.7 ms   20616 MiB/s  (1.15〜1.25x vs scalar)

horner scalar  :  51.5 ms    3880 MiB/s
horner dispatch:  14.8 ms   13521 MiB/s  (2.70〜3.52x vs scalar)
```

**About the variance in the measured values (an honest record)**: running the
benchmark four times in a row, the AVX2 speedup for GF multiplication varied in
the range of **11.6–18.1x**, Horner's method **2.70–3.52x**, and XOR
**1.12–1.25x** (the scalar side was stable at around 207 ms). The SIMD side
reaches 10,000–20,000 MiB/s and is therefore **limited by memory bandwidth**,
which makes it sensitive to cache state and to other processes. The table above
simply shows the result of a single run; it is more accurate to think of the
speedups as ranges.

- **GF(2^8) multiplication by an arbitrary coefficient is 11.6–18.1x faster with
  AVX2 than scalar (measured)**. This helps in the RAID6 Q parity and recovery
  paths.
- **Horner's method is 2.70–3.52x (measured)**. Because the scalar version is
  already optimized with u64 bit tricks, the difference is not as large as for
  GF multiplication.
- **Plain XOR is 1.12–1.25x**. Since even the scalar (u64) version is already
  pinned to memory bandwidth, there is little room for SIMD to help (as
  expected).
- PCLMULQDQ is 1.90x, and is only meaningful as a **fallback for older CPUs that
  cannot use AVX2**.

## Verification status (honest disclosure)

- ✅ **Scalar / PCLMULQDQ / AVX2**: execution-verified on the development machine
  above. In `cargo test`, the correctness of the scalar implementation is checked
  against a naive bit-shift implementation, and then the outputs of the
  PCLMULQDQ implementation (**all 256 coefficients × 9 lengths**) and the AVX2
  implementation (8 coefficients × 11 lengths, including remainder handling) are
  confirmed to match the scalar implementation. All 15 tests + 2 doctests pass.
- ⚠️ **The AVX-512 path is execution-unverified**. Because the development
  machine (Ryzen 9 3950X) does not have AVX-512, **only the fact that it
  compiles** has been confirmed. For safety it is not selected by the default
  dispatch, and it is enabled only as an opt-in when the environment variable
  `OPEN_CPU_ENABLE_AVX512=1` is set. This treatment will be maintained until
  verification on an AVX-512-equipped machine is complete.
- ⚠️ POPCNT/BMI1/BMI2/FMA/AES-NI/SHA-NI/VNNI **only have detection fields**;
  there is not yet any computational implementation that uses them.

## Running tests and benchmarks

```
cargo build --release
cargo test --release
cargo run --release --example bench
```

## Adoption record (as of 2026-08-22)

| Repository | How it is used |
|---|---|
| [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z) | The CPU feature detection, GF(2^8) multiplication, XOR, and Horner's method (AVX2 path) in `zfs_accel_hlsl/src/simd.rs` were delegated to this crate. After the migration all 39 existing tests still pass and the numbers match exactly. |
| [`open-english`](https://github.com/aon-co-jp/open-english) | The one-line summary in the server startup log, and `GET /v1/cpu-runtime` (returns the CPU instruction sets of the execution platform as JSON). |

Not yet adopted (future targets): `aruaru-db` (checksums, compression),
`aruaru-llm` (matrix operations), `open-cuda` (CPU fallback when no GPU is
present).

## Related

- Migration procedure: [PORTING.md](PORTING-US_English.md)
- Development policy and HANDOFF: [CLAUDE.md](CLAUDE-US_English.md)
- GitHub organization: https://github.com/aon-co-jp


---

## Update 2026-08-23 — Multi-ISA combination dispatch, plus FMA3/POPCNT/BMI kernels

Real CPUs ship **several instruction sets at once** (AVX2 *and* FMA3;
AVX-512F *and* BW *and* VNNI). A flat list of booleans forced every caller
to hand-write "only if both are present" checks, so `open-cpu` now models
combinations directly. Existing fields such as `caps.avx2` are unchanged,
so this is backwards compatible.

### New combination API (`src/isa.rs`)

| API | Purpose |
|---|---|
| `Feature` | Enum of 17 instruction sets, with string conversion (`Feature::from_name("AVX-VNNI")`) |
| `FeatureSet` | Bitmask set with `contains_all` / `contains_any` / union / intersection / difference |
| `CpuCapabilities::supports_all(&[Feature::Avx2, Feature::Fma])` | **The core combination check** |
| `IsaProfile` | Eight named tiers: `baseline`, `sse2`, `ssse3+pclmul`, `avx2`, `avx2+fma3`, `avx2+fma3+vnni`, `avx512f+bw+vl`, `avx512f+bw+vl+vnni` |
| `CpuCapabilities::isa_profile()` / `at_least(..)` | Highest satisfied tier / "at least this tier?" |
| `CpuCapabilities::detected_but_unused()` | Features detected but not yet exploited — keeps the project honest |
| `select(&[(tag, &[Feature])])` | Pick the first candidate path whose required combination is fully present |
| `vendor_family()` / `fast_bmi2()` | CPUID vendor+family, and whether `pext`/`pdep` are actually fast here |

```rust
use open_cpu::{Feature, IsaProfile};
let caps = open_cpu::detect();
if caps.supports_all(&[Feature::Avx2, Feature::Fma]) { /* vfmadd path */ }
if caps.at_least(IsaProfile::Avx512) { /* 512-bit path */ }
```

The tier model follows llama.cpp's "named variant" approach (a variant is a
*bundle* of features) rather than a combinatorial matrix of individual flags.

### New arithmetic and bit kernels (`src/math.rs`)

FMA3, POPCNT, BMI1 and BMI2 previously had detection fields but no code
using them. They now have real implementations, each verified against a
scalar reference (including non-multiple-of-vector-width lengths).

| API | Dispatch | Measured speedup |
|---|---|---|
| `dot_f32(a, b)` | AVX-512F (opt-in) / **AVX2+FMA3** / AVX2 / scalar | **3.17x** (4.23 → 13.40 GFLOP/s) |
| `axpy_f32(acc, src, scale)` | same | **1.03x** — memory-bandwidth bound, reported honestly |
| `scale_f32(dst, scale)` | AVX2 / scalar | not separately benchmarked |
| `popcount_bytes(data)` | POPCNT / scalar | **30.9x** |
| `hamming_distance(a, b)` | POPCNT / scalar | **11.0x** |
| `extract_bits` / `deposit_bits` | BMI2 *only if fast* / scalar | see warning below |
| `trailing_zeros_u64(v)` | BMI1 `tzcnt` / scalar | — |

### ⚠️ A feature bit being set does not mean the instruction is fast

On AMD Zen / Zen+ / Zen 2 (CPUID family 17h) and Hygon Dhyana, `pext` and
`pdep` are microcoded and dramatically slower than a scalar loop. AMD's
optimization guide for family 19h (Zen 3) states they became native ALU
operations (1/cycle throughput, 3-cycle latency) and explicitly advises
software with fast/slow paths to take the fast path on family 19h.

Measured on this development machine (Ryzen 9 3950X, family 17h):

```
pext scalar :  177.469 ms
pext bmi2   : 1268.991 ms   (0.14x — the hardware instruction is 7.1x SLOWER)
```

Naively writing "BMI2 is available, so use it" would have been a **7x
performance regression**. `open-cpu` therefore gates `extract_bits` /
`deposit_bits` on `fast_bmi2()`, which reads CPUID vendor and family.
Reproduce with `cargo run --release --example bench`.

### Detection-only additions

`gfni` and `vpclmulqdq` are now detected. Intel ISA-L 2.32 moved GF(2^8)
multiply to GFNI (`vgf2p8mulb`) and CRC to VPCLMULQDQ, which is the clear
next step for this crate's RAID6 kernels — but this machine has neither, so
no kernels were written for them rather than shipping unverifiable code.

### Test status

`cargo test --release`: **28 unit tests + 5 doctests pass, zero warnings.**

### Adopters after this change

| Repository | Usage |
|---|---|
| `open-raid-z` | GF(2^8), XOR and Horner kernels delegated here (47 tests, no regression) |
| `open-cuda` | `opencuda-blas` detection unified here; two real dispatch bugs fixed |
| `aruaru-llm` | `GET /v1/runtime` now reports the selected CPU SIMD combination |
| `open-english` | `GET /v1/cpu-runtime` reports combinations — **display only**, no hot loop exists there |
| `open-cg-cad` | Cross-section derivative rewritten via `axpy_f32`, **105.6x faster at n=2000** |

`open-fudousan` and `open-koumuten` were investigated and found to have no
CPU-bound work at all (small CRUD/web apps); no dependency was added there.

> Honesty note: figures above are measured on the development machine
> (AMD Ryzen 9 3950X, Zen 2). **AVX-512, AVX-VNNI, AVX-512 VNNI, GFNI and
> VPCLMULQDQ paths remain unverified on real hardware** because this CPU
> does not have them. AVX-512 code paths are never selected by default;
> they require `OPEN_CPU_ENABLE_AVX512=1`.
