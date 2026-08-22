> 日文原文 / 日本語原文: [PORTING.md](../PORTING.md)

# PORTING.md — 从其他仓库迁移到 `open-cpu` 的步骤

用于把 `open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` 等各自持有的
CPU 特性检测代码、GF(2^8) 运算代码集中到 `open-cpu` 的迁移步骤。

## 0. 前提

`open-cpu` 是 **库 crate**,不是常驻服务。
无需进程间通信或启动其他进程,只要在 `Cargo.toml` 中添加依赖并
调用函数即可完成。

## 1. 添加依赖

在本地工作驱动器 `F:\runo` 之下,path 依赖最为简便:

```toml
[dependencies]
open-cpu = { path = "../open-cpu" }
```

若要从工作空间内的成员 crate 使用,建议在工作空间根目录的
`Cargo.toml` 中写

```toml
[workspace.dependencies]
open-cpu = { path = "../open-cpu" }
```

并在成员一侧写 `open-cpu = { workspace = true }`。

将来若要切换为 git 依赖:

```toml
open-cpu = { git = "https://github.com/aon-co-jp/open-cpu", branch = "main" }
```

crate 名称带连字符,为 `open-cpu`;从 Rust 引用时的路径带下划线,
为 `open_cpu`。

## 2. 替换 CPU 特性检测代码

迁移前(各仓库中常见的写法):

```rust
static HAS_AVX2: OnceLock<bool> = OnceLock::new();
fn has_avx2() -> bool {
    *HAS_AVX2.get_or_init(|| is_x86_feature_detected!("avx2"))
}
```

迁移后:

```rust
if open_cpu::detect().avx2 { /* ... */ }
```

`detect()` 内部已使用 `OnceLock` 缓存,因此调用方无需再做缓存。
由于返回的是 `&'static CpuCapabilities`,也不会发生内存分配。

可用的字段: `avx2` `avx512f` `avx512bw` `avx512vl` `pclmulqdq`
`bmi1` `bmi2` `fma` `aes` `popcnt` `sha` `sse2` `ssse3` `avx_vnni` `avx512vnni`。

## 3. 替换 GF(2^8) / 校验运算(主要针对 `open-raid-z`)

`open-cpu` 的不可约多项式为 `0x11d`,生成元为 `g = 2`,与 Linux md/RAID6 以及
ZFS RAID-Z 相同。**迁移前务必确认自己仓库的多项式与生成元是否与之一致。**
若不同,数值就会对不上。

| 迁移前常见的写法 | 迁移后 |
|---|---|
| `for i in .. { p[i] ^= d[i] }` | `open_cpu::gf_xor(&mut p, &d)` |
| `for i in .. { q[i] ^= gf_mul(d[i], c) }` | `open_cpu::gf_mul_parity(&mut q, &d, c)` |
| `for i in .. { acc[i] = mul2(acc[i]) ^ d[i] }` | `open_cpu::gf_mul2_xor(&mut acc, &d)` |
| `for i in .. { acc[i] = mul4(acc[i]) ^ d[i] }` | `open_cpu::gf_mul4_xor(&mut acc, &d)` |
| 任意次数的 `×2^n` 霍纳法 | `open_cpu::gf_mul_pow2_xor(&mut acc, &d, n)` |
| 自有的 `gf_mul(a: u8, b: u8) -> u8` | `open_cpu::gf_mul(a, b)`(`const fn`) |
| 自有的 `mul2_byte(b) -> u8` | `open_cpu::gf_mul2_byte(b)`(`const fn`) |
| 自有的 `g^i` 系数计算 | `open_cpu::raid6_coeff(i)` |
| P/Q 批量计算 | `open_cpu::raid6_parity(&stripes, &mut p, &mut q)` |
| P/Q/R 批量计算(相当于 RAID-Z3) | `open_cpu::raid6_parity3(&stripes, &mut p, &mut q, &mut r)` |

`gf_mul_parity` / `gf_xor` 在 `dst.len() != src.len()` 时会 panic。
请在调用方先把条带长度对齐。

若想显式调用特定实现(用于基准测试或验证目的):

- `open_cpu::gf_mul_parity_scalar(...)` / `gf_xor_scalar(...)` /
  `gf_mul_pow2_xor_scalar(...)` — safe
- `unsafe { open_cpu::gf_mul_parity_avx2(...) }` — 由调用方保证支持 AVX2
- `unsafe { open_cpu::gf_mul_parity_pclmul(...) }` — 同理,需 SSSE3+PCLMULQDQ
- `unsafe { open_cpu::gf_mul_parity_avx512(...) }` — **运行未验证**

## 4. 迁移后的验证(必须)

1. `cargo build` — 依赖解析与构建能够通过。
2. `cargo test` — **既有测试全部通过**。特别是要确认替换前后
   校验数据的字节序列完全一致。若既有测试中没有
   校验值的比较,应在迁移时追加。
3. 若在日志中输出一行 `open_cpu::runtime_summary()`,
   事后就能确认在实机上选择了哪个实现。

## 4.5 实际的迁移示例(`open-raid-z`,2026-08-22)

作为参考,列出首个迁移实例的要点:

- 把 `detect_level()` 内的 `std::is_x86_feature_detected!` 替换为
  对 `open_cpu::detect()` 的引用(`SimdLevel` 这一
  仓库特有的枚举类型原样保留,**只把其判定依据**
  移交给 open-cpu)。既有的调用方完全无需改动。
- 只把 `gf_mul_xor_into()` / `xor_into()` / `mul_pow2_xor_into()` 的
  **AVX2 路径**委托给 open-cpu,而 AVX-512 路径(open-cpu 一侧也
  未验证)与 SSE2 路径(open-cpu 中没有实现)则保留了仓库一侧的实现。
  这是基于**不做会降低性能的替换**的判断。
- 不再被使用的 SIMD kernel 没有删除,而是加上 `#[allow(dead_code)]`
  保留下来,作为将来与 open-cpu 一侧交叉验证时的参照。

推荐采取这种「不一次性全部替换,而是从等价且性能不下降的部分开始逐步
委托」的推进方式。

## 5. 注意事项

- **AVX-512 路径运行未验证**(开发机不支持)。由于默认不会被选择,
  迁移不会导致行为发生变化。仅在 AVX-512 机器上进行验证时才
  设置 `OPEN_CPU_ENABLE_AVX512=1`。
- POPCNT/BMI/FMA/AES-NI/SHA-NI/VNNI **仅有检测**。使用它们的
  校验和、压缩、矩阵运算的实现在 `open-cpu` 中尚不存在,因此
  `aruaru-db` / `aruaru-llm` 一侧的相关代码还无法迁移
  (只先迁移检测部分是可行的)。
- 在 x86 以外(ARM 等)所有特性均为 `false`,会回退到标量实现。
  交叉构建可以通过,但尚不支持 NEON 等优化。
