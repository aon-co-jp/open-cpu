> Japanese original / 日本語原文: [PORTING.md](../PORTING.md)

# PORTING.md — Procedure for switching over to `open-cpu` from other repositories

A migration procedure for consolidating the CPU feature detection code and
GF(2^8) operation code that `open-raid-z` / `aruaru-db` / `aruaru-llm` /
`open-cuda` and others hold individually into `open-cpu`.

## 0. Prerequisites

`open-cpu` is a **library crate**, not a resident service. No inter-process
communication or launching of a separate process is needed; everything is done
by adding the dependency to `Cargo.toml` and calling functions.

## 1. Adding the dependency

Under the local working drive `F:\runo`, a path dependency is the simplest:

```toml
[dependencies]
open-cpu = { path = "../open-cpu" }
```

When using it from a member crate inside a workspace, it is preferable to write

```toml
[workspace.dependencies]
open-cpu = { path = "../open-cpu" }
```

in the workspace root's `Cargo.toml`, and write `open-cpu = { workspace = true }`
on the member side.

If you switch to a git dependency in the future:

```toml
open-cpu = { git = "https://github.com/aon-co-jp/open-cpu", branch = "main" }
```

The crate name is `open-cpu` with a hyphen; the path used when referring to it
from Rust is `open_cpu` with an underscore.

## 2. Replacing CPU feature detection code

Before migration (a pattern commonly found in each repository):

```rust
static HAS_AVX2: OnceLock<bool> = OnceLock::new();
fn has_avx2() -> bool {
    *HAS_AVX2.get_or_init(|| is_x86_feature_detected!("avx2"))
}
```

After migration:

```rust
if open_cpu::detect().avx2 { /* ... */ }
```

`detect()` is already `OnceLock`-cached internally, so there is no need to cache
it again on the calling side. Since it returns `&'static CpuCapabilities`, no
allocation occurs either.

Available fields: `avx2` `avx512f` `avx512bw` `avx512vl` `pclmulqdq`
`bmi1` `bmi2` `fma` `aes` `popcnt` `sha` `sse2` `ssse3` `avx_vnni` `avx512vnni`.

## 3. Replacing GF(2^8) / parity operations (mainly `open-raid-z`)

`open-cpu`'s irreducible polynomial is `0x11d` and its generator is `g = 2`,
identical to Linux md/RAID6 and ZFS RAID-Z. **Before migrating, be sure to check
whether your own repository's polynomial and generator match these.** If they
differ, the numbers will not agree.

| Common form before migration | After migration |
|---|---|
| `for i in .. { p[i] ^= d[i] }` | `open_cpu::gf_xor(&mut p, &d)` |
| `for i in .. { q[i] ^= gf_mul(d[i], c) }` | `open_cpu::gf_mul_parity(&mut q, &d, c)` |
| `for i in .. { acc[i] = mul2(acc[i]) ^ d[i] }` | `open_cpu::gf_mul2_xor(&mut acc, &d)` |
| `for i in .. { acc[i] = mul4(acc[i]) ^ d[i] }` | `open_cpu::gf_mul4_xor(&mut acc, &d)` |
| Horner's method with an arbitrary number of `×2^n` | `open_cpu::gf_mul_pow2_xor(&mut acc, &d, n)` |
| Your own `gf_mul(a: u8, b: u8) -> u8` | `open_cpu::gf_mul(a, b)` (`const fn`) |
| Your own `mul2_byte(b) -> u8` | `open_cpu::gf_mul2_byte(b)` (`const fn`) |
| Your own `g^i` coefficient computation | `open_cpu::raid6_coeff(i)` |
| Bulk P/Q computation | `open_cpu::raid6_parity(&stripes, &mut p, &mut q)` |
| Bulk P/Q/R computation (RAID-Z3 equivalent) | `open_cpu::raid6_parity3(&stripes, &mut p, &mut q, &mut r)` |

`gf_mul_parity` / `gf_xor` panic when `dst.len() != src.len()`. Make sure the
stripe lengths are aligned on the calling side.

If you want to call a specific implementation explicitly (for benchmarking or
verification purposes):

- `open_cpu::gf_mul_parity_scalar(...)` / `gf_xor_scalar(...)` /
  `gf_mul_pow2_xor_scalar(...)` — safe
- `unsafe { open_cpu::gf_mul_parity_avx2(...) }` — the caller guarantees AVX2 support
- `unsafe { open_cpu::gf_mul_parity_pclmul(...) }` — likewise for SSSE3+PCLMULQDQ
- `unsafe { open_cpu::gf_mul_parity_avx512(...) }` — **execution unverified**

## 4. Verification after migration (mandatory)

1. `cargo build` — dependency resolution and the build must succeed.
2. `cargo test` — **all existing tests must pass**. In particular, confirm that
   the parity byte sequences match exactly before and after the replacement. If
   the existing tests do not compare parity values, add such a comparison during
   the migration.
3. Printing `open_cpu::runtime_summary()` as one line in the log lets you check
   afterwards which implementation was selected on the real machine.

## 4.5 An actual migration example (`open-raid-z`, 2026-08-22)

For reference, here are the key points of the first real migration:

- The `std::is_x86_feature_detected!` inside `detect_level()` was replaced with a
  reference to `open_cpu::detect()` (the repository-specific enum `SimdLevel` was
  left as is, and **only the material used for the decision** was moved to
  open-cpu). This means no changes to the existing call sites are required at
  all.
- **Only the AVX2 paths** of `gf_mul_xor_into()` / `xor_into()` /
  `mul_pow2_xor_into()` were delegated to open-cpu, while the AVX-512 path
  (unverified on the open-cpu side too) and the SSE2 path (open-cpu has no
  implementation) kept the repository's own implementations. The judgment was:
  **do not make a replacement that lowers performance**.
- SIMD kernels that were no longer used were not deleted but left in place with
  `#[allow(dead_code)]`, serving as a reference for future cross-validation
  against the open-cpu side.

This approach of "not replacing everything at once, but delegating step by step
starting from the parts that are equivalent and do not lose performance" is
recommended.

## 5. Points to note

- **The AVX-512 path is execution-unverified** (the development machine does not
  have it). It is not selected by default, so the migration will not change
  behavior. Set `OPEN_CPU_ENABLE_AVX512=1` only when verifying on an AVX-512
  machine.
- POPCNT/BMI/FMA/AES-NI/SHA-NI/VNNI are **detection only**. Implementations of
  checksums, compression, and matrix operations that use them do not exist in
  `open-cpu` yet, so the corresponding code on the `aruaru-db` / `aruaru-llm`
  side cannot be migrated yet (migrating only the detection part first is
  possible).
- On non-x86 platforms (ARM, etc.) all features become `false` and the code falls
  back to the scalar implementation. Cross-building succeeds, but optimizations
  such as NEON are not supported.
