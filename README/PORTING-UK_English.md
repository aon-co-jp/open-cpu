> Japanese original / 日本語原文: [PORTING.md](../PORTING.md)

# PORTING.md — the procedure for switching over to `open-cpu` from another repository

The migration procedure for consolidating into `open-cpu` the CPU feature
detection code and GF(2^8) arithmetic code that `open-raid-z` / `aruaru-db` /
`aruaru-llm` / `open-cuda` and others hold individually.

## 0. Prerequisites

`open-cpu` is a **library crate**, not a resident service. No inter-process
communication or launching of a separate process is needed; everything is complete
once you add the dependency to `Cargo.toml` and call the functions.

## 1. Adding the dependency

Under the local working drive `F:\runo`, a path dependency is the simplest:

```toml
[dependencies]
open-cpu = { path = "../open-cpu" }
```

When using it from a member crate inside a workspace, it is preferable to write
the following in the workspace root's `Cargo.toml`

```toml
[workspace.dependencies]
open-cpu = { path = "../open-cpu" }
```

and to write `open-cpu = { workspace = true }` on the member side.

If you switch to a git dependency in the future:

```toml
open-cpu = { git = "https://github.com/aon-co-jp/open-cpu", branch = "main" }
```

The crate name is hyphenated, `open-cpu`, while the path used to refer to it from
Rust is underscored, `open_cpu`.

## 2. Replacing the CPU feature detection code

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

`detect()` already caches internally with a `OnceLock`, so there is no need for the
caller to cache it again. Because it returns a `&'static CpuCapabilities`, no
allocation occurs either.

Available fields: `avx2` `avx512f` `avx512bw` `avx512vl` `pclmulqdq`
`bmi1` `bmi2` `fma` `aes` `popcnt` `sha` `sse2` `ssse3` `avx_vnni` `avx512vnni`.

## 3. Replacing GF(2^8) / parity arithmetic (mainly `open-raid-z`)

`open-cpu`'s irreducible polynomial is `0x11d` and its generator is `g = 2`, the
same as Linux md/RAID6 and ZFS RAID-Z. **Before migrating, always check whether
your own repository's polynomial and generator match these.** If they differ, the
numbers will no longer agree.

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
| Bulk P/Q/R computation (equivalent to RAID-Z3) | `open_cpu::raid6_parity3(&stripes, &mut p, &mut q, &mut r)` |

`gf_mul_parity` / `gf_xor` panic when `dst.len() != src.len()`. Make sure the
stripe lengths are aligned on the caller's side.

If you want to call a specific implementation explicitly (for benchmarking or
verification purposes):

- `open_cpu::gf_mul_parity_scalar(...)` / `gf_xor_scalar(...)` /
  `gf_mul_pow2_xor_scalar(...)` — safe
- `unsafe { open_cpu::gf_mul_parity_avx2(...) }` — the caller guarantees AVX2
  support
- `unsafe { open_cpu::gf_mul_parity_pclmul(...) }` — likewise SSSE3+PCLMULQDQ
- `unsafe { open_cpu::gf_mul_parity_avx512(...) }` — **not verified in execution**

## 4. Verification after migration (mandatory)

1. `cargo build` — dependency resolution and the build must succeed.
2. `cargo test` — **all existing tests must pass**. In particular, confirm that
   the parity byte sequences match exactly before and after the replacement. If
   the existing tests do not compare parity values, add such a comparison during
   the migration.
3. Emitting `open_cpu::runtime_summary()` as a single line in the log lets you
   check afterwards which implementation was selected on the actual machine.

## 4.5 An actual migration example (`open-raid-z`, 2026-08-22)

For reference, here are the key points of the first actual migration:

- The `std::is_x86_feature_detected!` inside `detect_level()` was replaced with a
  reference to `open_cpu::detect()` (the repository-specific enum `SimdLevel` was
  left as it was, and **only the material used for the decision** was moved to
  open-cpu). No change to the existing call sites was needed at all.
- **Only the AVX2 paths** of `gf_mul_xor_into()` / `xor_into()` /
  `mul_pow2_xor_into()` were delegated to open-cpu, while the AVX-512 path
  (unverified on the open-cpu side too) and the SSE2 path (no implementation in
  open-cpu) kept the repository's own implementations. The judgement was:
  **do not make a replacement that lowers performance**.
- The SIMD kernels that were no longer used were not deleted but left in place
  with `#[allow(dead_code)]` attached, serving as a reference for future
  cross-validation against the open-cpu side.

This approach of "not replacing everything at once, but delegating step by step
starting from the parts that are equivalent and do not lose performance" is
recommended.

## 5. Points to note

- **The AVX-512 path is not verified in execution** (the development machine does
  not have it). It is not selected by default, so migration will not change
  behaviour. Set `OPEN_CPU_ENABLE_AVX512=1` only when verifying on an AVX-512
  machine.
- POPCNT/BMI/FMA/AES-NI/SHA-NI/VNNI are **detection only**. `open-cpu` does not
  yet have implementations of checksums, compression or matrix arithmetic that use
  them, so the corresponding code on the `aruaru-db` / `aruaru-llm` side cannot be
  migrated yet (migrating just the detection part first is possible).
- On anything other than x86 (ARM, etc.) all features become `false` and the code
  falls back to the scalar implementation. Cross-builds succeed, but optimisations
  such as NEON are not supported.
