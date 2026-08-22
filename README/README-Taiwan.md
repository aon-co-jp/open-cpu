> 日文原文 / 日本語原文: [README.md](../README.md)

# open-cpu

`aon-co-jp` 生態系共用的 **CPU 指令集偵測與執行期分派函式庫**(Rust)。

為了避免 `open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` 各自撰寫
重複的 CPU 功能偵測程式碼而新設。

**它並非常駐服務(daemon)。** 它是各儲存庫在 `Cargo.toml` 的
`[dependencies]` 中加入、連結進同一個行程內使用的一般函式庫 crate。

## 功能

1. **CPU 功能的執行期偵測** — `open_cpu::detect()` 會回傳
   `&'static CpuCapabilities`。內部使用 `std::is_x86_feature_detected!`,
   並將首次偵測結果快取於 `OnceLock`,因此不論呼叫幾次成本都近乎於零。
2. **RAID6 GF(2^8) 運算的執行期分派** — 依偵測結果在執行期選擇
   純量 / PCLMULQDQ / AVX2 / AVX-512 的實作。

## 使用方式

```toml
# 依存側の Cargo.toml
[dependencies]
open-cpu = { path = "../open-cpu" }
```

```rust
// 1. CPU 機能検出
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

### 公開 API 一覽

| API | 內容 | 分派 |
|---|---|---|
| `detect() -> &'static CpuCapabilities` | CPU 功能偵測(`OnceLock` 快取) | — |
| `runtime_summary() -> String` | 偵測結果+所選實作的一行摘要 | — |
| `selected_impl() -> GfImpl` | GF 運算中所選用的實作 | — |
| `gf_xor(dst, src)` | `dst ^= src`(P 同位) | AVX2 / 純量 |
| `gf_mul_parity(dst, src, factor)` | `dst ^= src * factor`(Q 同位) | AVX-512(需自行啟用)/ AVX2 / PCLMULQDQ / 純量 |
| `gf_mul_pow2_xor(acc, src, times)` | `acc = acc * 2^times ^ src` | AVX2 / 純量 |
| `gf_mul2_xor` / `gf_mul4_xor` | 上述的 `times=1` / `times=2` | 同上 |
| `raid6_parity(stripes, p, q)` | 以係數表方式一次計算 P/Q | 比照上述 |
| `raid6_parity3(stripes, p, q, r)` | 以霍納法一次計算 P/Q/R | 比照上述 |
| `gf_mul(a, b) -> u8` | 單一位元組的 GF 乘法(`const fn`) | — |
| `gf_mul2_byte(b) -> u8` | 單一位元組在 GF 上乘 2(`const fn`) | — |
| `raid6_coeff(i) -> u8` | RAID6 的係數 `g^i`(`g = 2`) | — |

亦公開可明確呼叫各實作的 `*_scalar` / `*_avx2` / `*_pclmul` / `*_avx512`
版本(供效能量測與交叉驗證使用,SIMD 版為 `unsafe`)。

## 偵測對象的指令集

| 指令集 | 偵測 | 在本 crate 中的運用 |
|---|---|---|
| SSE2 | ✅ | 作為 PCLMULQDQ 路徑的輔助使用 |
| SSSE3 | ✅ | `pshufb`(PCLMULQDQ 路徑的約簡) |
| PCLMULQDQ | ✅ | GF(2^8) 乘法(無進位乘法實作) |
| AVX2 | ✅ | GF(2^8) 乘法(`vpshufb` split-table,預設選用) |
| AVX-512F / BW / VL | ✅ | 有 GF(2^8) 乘法路徑(**執行未驗證**,詳見下文) |
| POPCNT | ✅ | 僅偵測(沒有使用它的實作) |
| BMI1 / BMI2 | ✅ | 僅偵測(沒有使用它的實作) |
| FMA3 | ✅ | 僅偵測(沒有使用它的實作) |
| AES-NI | ✅ | 僅偵測(沒有使用它的實作) |
| SHA-NI | ✅ | 僅偵測(沒有使用它的實作) |
| AVX-VNNI | ✅ | 僅偵測(供未來的 AI 推論使用,沒有使用它的實作) |
| AVX-512 VNNI | ✅ | 僅偵測(供未來的 AI 推論使用,沒有使用它的實作) |

在 x86/x86_64 以外的架構上,所有欄位都會是 `false`,並回退到
純量實作(可以正常建置)。

## GF(2^8) 實作細節

不可約多項式為 `0x11d`(x^8+x^4+x^3+x^2+1),生成元為 `g = 2`。
與 Linux md/RAID6 以及 ZFS RAID-Z 相同。

- **純量**: 以 nibble split 表(16 筆 × 2)構成的參考實作。
- **PCLMULQDQ**: 將各位元組以 16bit 間隔展開後,與 8bit 係數的無進位乘積
  (最大 15bit)不會進位到相鄰的槽位。利用此性質,以 1 道指令一次完成
  4 個位元組的乘法,再以 2 次 `pshufb` 約簡至 GF(2^8)。16 byte/iter。
- **AVX2**: 以 `vpshufb` 實作的 split-table。32 byte/iter。
- **AVX-512F/BW**: 以 64 byte/iter 處理相同的 split-table。

## 實測效能量測

`cargo run --release --example bench`(4 MiB × 50 次 = 200 MiB,factor=0x8d)

開發機: **AMD Ryzen 9 3950X** / Windows 11 / rustc 1.96.0

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

**關於實測值的變動(誠實的記錄)**: 連續執行 4 次的結果顯示,
GF 乘法的 AVX2 倍率在 **11.6〜18.1 倍**、霍納法在 **2.70〜3.52 倍**、
XOR 在 **1.12〜1.25 倍** 的範圍內變動(純量側則穩定在 207 ms 前後)。
SIMD 側已達到 1 萬〜2 萬 MiB/s,屬於**記憶體頻寬受限**的狀態,
因此容易受到快取狀態與其他行程的影響。上表是直接刊載某一次執行的結果,
將倍率視為一個範圍來理解才正確。

- **GF(2^8) 任意係數乘法在 AVX2 下為純量的 11.6〜18.1 倍(實測)**。
  在 RAID6 的 Q 同位與復原路徑上很有效。
- **霍納法為 2.70〜3.52 倍(實測)**。由於純量版本已經以 u64 位元
  技巧最佳化過,因此差距不像 GF 乘法那麼大。
- **單純的 XOR 為 1.12〜1.25 倍**。在純量(u64)階段就已經緊貼記憶體頻寬,
  SIMD 化的空間很小(與預期相符)。
- PCLMULQDQ 為 1.90 倍,只有作為**針對無法使用 AVX2 的舊 CPU 的回退方案**
  才具有意義。

## 驗證狀況(誠實揭露)

- ✅ **純量 / PCLMULQDQ / AVX2**: 已在上述開發機上完成執行驗證。
  在 `cargo test` 中,以樸素的位移實作為基準確認純量實作的正確性,
  再以純量實作為基準,確認 PCLMULQDQ 實作(**全部 256 種係數 × 9 種
  長度**)與 AVX2 實作(8 種係數 × 11 種長度,含畸零部分的處理)的
  輸出一致。全部 15 項測試 + 2 件 doctest 通過。
- ⚠️ **AVX-512 路徑尚未經執行驗證**。由於開發機(Ryzen 9 3950X)並未搭載
  AVX-512,**僅確認可以通過編譯**。為了安全起見,預設的分派不會選用它,
  只有在設定環境變數 `OPEN_CPU_ENABLE_AVX512=1` 時才會以自行啟用的方式生效。
  在搭載 AVX-512 的機器上完成驗證之前,將維持此一處置方式。
- ⚠️ POPCNT/BMI1/BMI2/FMA/AES-NI/SHA-NI/VNNI **只有偵測欄位**,
  尚未有使用它們的運算實作。

## 測試與效能量測的執行

```
cargo build --release
cargo test --release
cargo run --release --example bench
```

## 導入實績(截至 2026-08-22)

| 儲存庫 | 使用方式 |
|---|---|
| [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z) | 將 `zfs_accel_hlsl/src/simd.rs` 的 CPU 功能偵測、GF(2^8) 乘法、XOR、霍納法(AVX2 路徑)委由本 crate 處理。移轉後既有的 39 項測試全數通過,數值完全一致。 |
| [`open-english`](https://github.com/aon-co-jp/open-english) | 伺服器啟動記錄的一行摘要,以及 `GET /v1/cpu-runtime`(以 JSON 回傳執行環境的 CPU 指令集)。 |

尚未導入(今後的對象): `aruaru-db`(檢查碼、壓縮)、
`aruaru-llm`(矩陣運算)、`open-cuda`(無 GPU 時的 CPU 回退)。

## 相關

- 移轉步驟: [PORTING.md](PORTING-Taiwan.md)
- 開發方針與 HANDOFF: [CLAUDE.md](CLAUDE-Taiwan.md)
- GitHub organization: https://github.com/aon-co-jp
