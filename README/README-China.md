> 日文原文 / 日本語原文: [README.md](../README.md)

# open-cpu

`aon-co-jp` 生态系统通用的 **CPU 指令集检测与运行时分派库**(Rust)。

为避免 `open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` 各自编写重复的
CPU 特性检测代码而新建。

**它不是常驻服务(守护进程)。** 它是各仓库在 `Cargo.toml` 的
`[dependencies]` 中添加、链接进同一进程内使用的普通库 crate。

## 功能

1. **CPU 特性的运行时检测** — `open_cpu::detect()` 返回
   `&'static CpuCapabilities`。内部使用 `std::is_x86_feature_detected!`,
   并把首次检测结果缓存在 `OnceLock` 中,因此无论调用多少次开销都接近于零。
2. **RAID6 GF(2^8) 运算的运行时分派** — 根据检测结果在运行时选择
   标量 / PCLMULQDQ / AVX2 / AVX-512 的实现。

## 使用方法

```toml
# 依赖方的 Cargo.toml
[dependencies]
open-cpu = { path = "../open-cpu" }
```

```rust
// 1. CPU 机能検出
let caps = open_cpu::detect();
println!("{}", caps);   // Display 実装あり
// => sse2 ssse3 popcnt aes pclmulqdq bmi1 bmi2 fma sha avx2

if caps.avx2 { /* ... */ }
if caps.has_all(&[caps.avx2, caps.fma]) { /* AVX2+FMA3 の経路 */ }

// 2. RAID6 パリティ(係数テーブル方式)
let d0 = vec![1u8; 4096];
let d1 = vec![2u8; 4096];
let mut p = vec![0u8; 4096];
let mut q = vec![0u8; 4096];
open_cpu::raid6_parity(&[&d0, &d1], &mut p, &mut q);

// 3. RAID-Z3 相当の P/Q/R(ホーナー法、係数テーブル不要で高速)
let mut r = vec![0u8; 4096];
open_cpu::raid6_parity3(&[&d0, &d1], &mut p, &mut q, &mut r);

// 個別 API
open_cpu::gf_xor(&mut p, &d0);                 // p ^= d0          (P パリティ)
open_cpu::gf_mul_parity(&mut q, &d0, 0x02);    // q ^= d0 * 0x02   (Q パリティ)
open_cpu::gf_mul2_xor(&mut q, &d0);            // q = q*2 ^ d0     (ホーナー法)
open_cpu::gf_mul4_xor(&mut r, &d0);            // r = r*4 ^ d0     (ホーナー法)

// ログ 1 行サマリ
println!("{}", open_cpu::runtime_summary());
// => open-cpu 0.1.0 | features: ... | gf impl: Avx2
```

### 公开 API 一览

| API | 内容 | 分派 |
|---|---|---|
| `detect() -> &'static CpuCapabilities` | CPU 特性检测(`OnceLock` 缓存) | — |
| `runtime_summary() -> String` | 检测结果 + 所选实现的单行摘要 | — |
| `selected_impl() -> GfImpl` | GF 运算中所选的实现 | — |
| `gf_xor(dst, src)` | `dst ^= src`(P 校验) | AVX2 / 标量 |
| `gf_mul_parity(dst, src, factor)` | `dst ^= src * factor`(Q 校验) | AVX-512(需显式启用)/ AVX2 / PCLMULQDQ / 标量 |
| `gf_mul_pow2_xor(acc, src, times)` | `acc = acc * 2^times ^ src` | AVX2 / 标量 |
| `gf_mul2_xor` / `gf_mul4_xor` | 上述的 `times=1` / `times=2` | 同上 |
| `raid6_parity(stripes, p, q)` | 以系数表方式批量计算 P/Q | 同上 |
| `raid6_parity3(stripes, p, q, r)` | 以霍纳法批量计算 P/Q/R | 同上 |
| `gf_mul(a, b) -> u8` | 单字节 GF 乘法(`const fn`) | — |
| `gf_mul2_byte(b) -> u8` | 单字节在 GF 上乘 2(`const fn`) | — |
| `raid6_coeff(i) -> u8` | RAID6 的系数 `g^i`(`g = 2`) | — |

还公开了显式调用各实现的 `*_scalar` / `*_avx2` / `*_pclmul` / `*_avx512`
版本(用于基准测试与交叉验证,SIMD 版为 `unsafe`)。

## 检测对象的指令集

| 指令集 | 检测 | 本 crate 中的使用情况 |
|---|---|---|
| SSE2 | ✅ | 作为 PCLMULQDQ 路径的辅助使用 |
| SSSE3 | ✅ | `pshufb`(PCLMULQDQ 路径的约简) |
| PCLMULQDQ | ✅ | GF(2^8) 乘法(无进位乘法实现) |
| AVX2 | ✅ | GF(2^8) 乘法(`vpshufb` split-table,默认选择) |
| AVX-512F / BW / VL | ✅ | 有 GF(2^8) 乘法路径(**运行未验证**,详见下文) |
| POPCNT | ✅ | 仅检测(无使用它的实现) |
| BMI1 / BMI2 | ✅ | 仅检测(无使用它的实现) |
| FMA3 | ✅ | 仅检测(无使用它的实现) |
| AES-NI | ✅ | 仅检测(无使用它的实现) |
| SHA-NI | ✅ | 仅检测(无使用它的实现) |
| AVX-VNNI | ✅ | 仅检测(面向未来的 AI 推理,无使用它的实现) |
| AVX-512 VNNI | ✅ | 仅检测(面向未来的 AI 推理,无使用它的实现) |

在 x86/x86_64 以外的架构上所有字段均为 `false`,并回退到标量实现
(构建可以通过)。

## GF(2^8) 实现细节

不可约多项式为 `0x11d`(x^8+x^4+x^3+x^2+1),生成元为 `g = 2`。
与 Linux md/RAID6 以及 ZFS RAID-Z 相同。

- **标量**: 基于 nibble split 表(16 项 × 2)的参考实现。
- **PCLMULQDQ**: 将各字节以 16bit 间隔展开后,与 8bit 系数的无进位乘积
  (最大 15bit)不会进位到相邻的槽位。利用这一性质,用 1 条指令一次完成
  4 个字节的乘法,再通过 2 次 `pshufb` 约简到 GF(2^8)。16 byte/iter。
- **AVX2**: 基于 `vpshufb` 的 split-table 实现。32 byte/iter。
- **AVX-512F/BW**: 以 64 byte/iter 处理同样的 split-table。

## 实测基准测试

`cargo run --release --example bench`(4 MiB × 50 次 = 200 MiB,factor=0x8d)

开发机: **AMD Ryzen 9 3950X** / Windows 11 / rustc 1.96.0

```
open-cpu 0.1.0 | features: sse2 ssse3 popcnt aes pclmulqdq bmi1 bmi2 fma sha avx2 | gf impl: Avx2
scalar   :  207.5 ms     963 MiB/s
pclmulqdq:  109.0 ms    1835 MiB/s  (1.90x vs scalar)
avx2     :   11.4 ms   17473 MiB/s  (18.14x vs scalar)
avx512   : このCPUでは未搭載のため実測不可(未検証)

xor scalar     :  11.5 ms   17451 MiB/s
xor dispatch   :   9.7 ms   20616 MiB/s  (1.15〜1.25x vs scalar)

horner scalar  :  51.5 ms    3880 MiB/s
horner dispatch:  14.8 ms   13521 MiB/s  (2.70〜3.52x vs scalar)
```

**关于实测值的波动(如实记录)**: 连续执行 4 次的结果显示,
GF 乘法的 AVX2 倍率在 **11.6〜18.1 倍**、霍纳法在 **2.70〜3.52 倍**、
XOR 在 **1.12〜1.25 倍** 的范围内波动(标量一侧稳定在 207 ms 前后)。
SIMD 一侧已达到 1 万〜2 万 MiB/s,处于**内存带宽受限**状态,
因此容易受缓存状态和其他进程的影响。上表原样刊载的是某一次执行的结果,
把倍率理解为一个范围才是准确的。

- **GF(2^8) 任意系数乘法在 AVX2 下为标量的 11.6〜18.1 倍(实测)**。
  在 RAID6 的 Q 校验与恢复路径上有效。
- **霍纳法为 2.70〜3.52 倍(实测)**。由于标量版本已用 u64 位运算
  技巧优化过,因此差距不像 GF 乘法那样大。
- **单纯的 XOR 为 1.12〜1.25 倍**。标量(u64)阶段就已经贴近内存带宽,
  SIMD 化的余地很小(与预期一致)。
- PCLMULQDQ 为 1.90 倍,只有作为**面向无法使用 AVX2 的老旧 CPU 的回退方案**
  才有意义。

## 验证状况(如实披露)

- ✅ **标量 / PCLMULQDQ / AVX2**: 已在上述开发机上完成运行验证。
  在 `cargo test` 中,以朴素的位移实现为基准确认了标量实现的正确性,
  并进一步以标量实现为基准,确认了 PCLMULQDQ 实现(**全部 256 种系数 × 9 种
  长度**)与 AVX2 实现(8 种系数 × 11 种长度,含尾数处理)的
  输出一致。全部 15 项测试 + 2 项 doctest 通过。
- ⚠️ **AVX-512 路径运行未验证**。由于开发机(Ryzen 9 3950X)不支持 AVX-512,
  **仅确认了能够编译通过**。为安全起见,默认分派不会选择它,
  只有在设置了环境变量 `OPEN_CPU_ENABLE_AVX512=1` 时才以显式启用的方式生效。
  在配备 AVX-512 的机器上完成验证之前,将维持这一处理方式。
- ⚠️ POPCNT/BMI1/BMI2/FMA/AES-NI/SHA-NI/VNNI **仅有检测字段**,
  尚无使用它们的运算实现。

## 测试与基准测试的执行

```
cargo build --release
cargo test --release
cargo run --release --example bench
```

## 采用实绩(截至 2026-08-22)

| 仓库 | 使用方式 |
|---|---|
| [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z) | 将 `zfs_accel_hlsl/src/simd.rs` 的 CPU 特性检测、GF(2^8) 乘法、XOR、霍纳法(AVX2 路径)委托给本 crate。迁移后既有的 39 项测试全部通过,数值完全一致。 |
| [`open-english`](https://github.com/aon-co-jp/open-english) | 服务器启动日志的单行摘要,以及 `GET /v1/cpu-runtime`(以 JSON 返回运行基础设施的 CPU 指令集)。 |

尚未采用(今后的对象): `aruaru-db`(校验和、压缩)、
`aruaru-llm`(矩阵运算)、`open-cuda`(无 GPU 时的 CPU 回退)。

## 相关

- 迁移步骤: [PORTING.md](PORTING-China.md)
- 开发方针与 HANDOFF: [CLAUDE.md](CLAUDE-China.md)
- GitHub organization: https://github.com/aon-co-jp
