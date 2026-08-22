> Japanese original / 日本語原文: [README.md](../README.md)

# open-cpu

A **CPU instruction set detection and runtime dispatch library** (Rust) shared
across the `aon-co-jp` ecosystem.

It was created to avoid `open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda`
each writing their own duplicated CPU feature detection code.

**It is not a resident service (daemon).** It is an ordinary library crate that
each repository adds to the `[dependencies]` section of its `Cargo.toml` and
links into the same process.

## What it can do

1. **Runtime detection of CPU features** — `open_cpu::detect()` returns a
   `&'static CpuCapabilities`. It uses `std::is_x86_feature_detected!` and caches
   the first detection result in a `OnceLock`, so the cost of calling it repeatedly
   is close to zero.
2. **Runtime dispatch of RAID6 GF(2^8) arithmetic** — depending on the detection
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
println!("{}", caps);   // a Display implementation is provided
// => sse2 ssse3 popcnt aes pclmulqdq bmi1 bmi2 fma sha avx2

if caps.avx2 { /* ... */ }
if caps.has_all(&[caps.avx2, caps.fma]) { /* the AVX2+FMA3 path */ }

// 2. RAID6 parity (coefficient table method)
let d0 = vec![1u8; 4096];
let d1 = vec![2u8; 4096];
let mut p = vec![0u8; 4096];
let mut q = vec![0u8; 4096];
open_cpu::raid6_parity(&[&d0, &d1], &mut p, &mut q);

// 3. P/Q/R equivalent to RAID-Z3 (Horner's method; fast, no coefficient table needed)
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
| `runtime_summary() -> String` | One-line summary of the detection result plus the selected implementation | — |
| `selected_impl() -> GfImpl` | The implementation selected for GF arithmetic | — |
| `gf_xor(dst, src)` | `dst ^= src` (P parity) | AVX2 / scalar |
| `gf_mul_parity(dst, src, factor)` | `dst ^= src * factor` (Q parity) | AVX-512 (opt-in) / AVX2 / PCLMULQDQ / scalar |
| `gf_mul_pow2_xor(acc, src, times)` | `acc = acc * 2^times ^ src` | AVX2 / scalar |
| `gf_mul2_xor` / `gf_mul4_xor` | The above with `times=1` / `times=2` | As above |
| `raid6_parity(stripes, p, q)` | Computes P/Q in bulk using the coefficient table method | As above |
| `raid6_parity3(stripes, p, q, r)` | Computes P/Q/R in bulk using Horner's method | As above |
| `gf_mul(a, b) -> u8` | Single-byte GF multiplication (`const fn`) | — |
| `gf_mul2_byte(b) -> u8` | Doubling of a single byte over GF (`const fn`) | — |
| `raid6_coeff(i) -> u8` | The RAID6 coefficient `g^i` (`g = 2`) | — |

`*_scalar` / `*_avx2` / `*_pclmul` / `*_avx512` versions that call each
implementation explicitly are also public (for benchmarking and cross-validation;
the SIMD versions are `unsafe`).

## Instruction sets covered by detection

| Instruction set | Detected | Use within this crate |
|---|---|---|
| SSE2 | ✅ | Used as a helper for the PCLMULQDQ path |
| SSSE3 | ✅ | `pshufb` (reduction in the PCLMULQDQ path) |
| PCLMULQDQ | ✅ | GF(2^8) multiplication (carry-less multiplication implementation) |
| AVX2 | ✅ | GF(2^8) multiplication (`vpshufb` split-table; selected by default) |
| AVX-512F / BW / VL | ✅ | A GF(2^8) multiplication path exists (**not verified in execution**, see below) |
| POPCNT | ✅ | Detection only (no implementation uses it) |
| BMI1 / BMI2 | ✅ | Detection only (no implementation uses it) |
| FMA3 | ✅ | Detection only (no implementation uses it) |
| AES-NI | ✅ | Detection only (no implementation uses it) |
| SHA-NI | ✅ | Detection only (no implementation uses it) |
| AVX-VNNI | ✅ | Detection only (aimed at future AI inference; no implementation uses it) |
| AVX-512 VNNI | ✅ | Detection only (aimed at future AI inference; no implementation uses it) |

On architectures other than x86/x86_64 every field becomes `false` and the code
falls back to the scalar implementation (the build still succeeds).

## Details of the GF(2^8) implementation

The irreducible polynomial is `0x11d` (x^8+x^4+x^3+x^2+1) and the generator is
`g = 2`. This is the same as Linux md/RAID6 and ZFS RAID-Z.

- **Scalar**: a reference implementation using nibble split tables
  (16 entries × 2).
- **PCLMULQDQ**: if each byte is expanded to 16-bit intervals, the carry-less
  product with an 8-bit coefficient (at most 15 bits) does not carry over into the
  adjacent slot. Exploiting this property, 4 bytes are multiplied together in a
  single instruction and reduced to GF(2^8) with two `pshufb` operations.
  16 bytes/iter.
- **AVX2**: a split-table implementation using `vpshufb`. 32 bytes/iter.
- **AVX-512F/BW**: the same split table processed at 64 bytes/iter.

## Measured benchmarks

`cargo run --release --example bench` (4 MiB × 50 runs = 200 MiB, factor=0x8d)

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

**About the variability of the measured values (an honest record)**: over four
consecutive runs, the AVX2 speedup for GF multiplication varied within the range
**11.6–18.1×**, Horner's method within **2.70–3.52×**, and XOR within
**1.12–1.25×** (the scalar side was stable at around 207 ms). The SIMD side reaches
10,000–20,000 MiB/s and is therefore **limited by memory bandwidth**, making it
susceptible to cache state and to other processes. The table above simply reports
the result of a single run; it is more accurate to think of the speedups as ranges.

- **GF(2^8) multiplication by an arbitrary coefficient is 11.6–18.1× faster than
  scalar with AVX2 (measured)**. This matters on the RAID6 Q parity and recovery
  paths.
- **Horner's method is 2.70–3.52× faster (measured)**. Because the scalar version
  is already optimised with u64 bit tricks, the difference is not as large as for
  GF multiplication.
- **Plain XOR is 1.12–1.25× faster**. Since even the scalar (u64) version is
  already pinned against memory bandwidth, there is little room for SIMD to help
  (as expected).
- PCLMULQDQ is 1.90×, and is only meaningful as **a fallback for older CPUs that
  cannot use AVX2**.

## Verification status (honest disclosure)

- ✅ **Scalar / PCLMULQDQ / AVX2**: verified in execution on the development
  machine above. In `cargo test`, the correctness of the scalar implementation is
  checked against a naive bit-shift implementation, and then the scalar
  implementation is used as the reference to confirm that the PCLMULQDQ
  implementation (**all 256 coefficients × 9 lengths**) and the AVX2
  implementation (8 coefficients × 11 lengths, including remainder handling)
  produce matching output. All 15 tests plus 2 doctests pass.
- ⚠️ **The AVX-512 path is not verified in execution**. Because the development
  machine (Ryzen 9 3950X) does not have AVX-512, **only the fact that it compiles**
  has been confirmed. For safety it is not selected by the default dispatch, and
  becomes enabled on an opt-in basis only when the environment variable
  `OPEN_CPU_ENABLE_AVX512=1` is set. This treatment will be maintained until
  verification on an AVX-512-capable machine is complete.
- ⚠️ POPCNT/BMI1/BMI2/FMA/AES-NI/SHA-NI/VNNI **merely have detection fields**;
  there is as yet no arithmetic implementation that uses them.

## Running the tests and benchmarks

```
cargo build --release
cargo test --release
cargo run --release --example bench
```

## Adoption record (as of 2026-08-22)

| Repository | How it is used |
|---|---|
| [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z) | The CPU feature detection, GF(2^8) multiplication, XOR and Horner's method (AVX2 path) in `zfs_accel_hlsl/src/simd.rs` have been delegated to this crate. All 39 existing tests still pass after the migration, and the numbers match exactly. |
| [`open-english`](https://github.com/aon-co-jp/open-english) | The one-line summary in the server start-up log, and `GET /v1/cpu-runtime` (which returns the CPU instruction sets of the execution platform as JSON). |

Not yet adopted (future targets): `aruaru-db` (checksums and compression),
`aruaru-llm` (matrix arithmetic), `open-cuda` (CPU fallback when no GPU is
present).

## Related

- Migration procedure: [PORTING.md](PORTING-UK_English.md)
- Development policy and HANDOFF: [CLAUDE.md](CLAUDE-UK_English.md)
- GitHub organisation: https://github.com/aon-co-jp
