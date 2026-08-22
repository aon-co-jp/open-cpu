> 日文原文 / 日本語原文: [PORTING.md](../PORTING.md)

# PORTING.md — 從其他儲存庫改用 `open-cpu` 的步驟

用於把 `open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` 等各自持有的
CPU 功能偵測程式碼、GF(2^8) 運算程式碼集中到 `open-cpu` 的移轉步驟。

## 0. 前提

`open-cpu` 是 **函式庫 crate**,並非常駐服務。
不需要行程間通訊或啟動其他行程,只要在 `Cargo.toml` 中加入相依並
呼叫函式即可完成。

## 1. 加入相依

在本機工作磁碟 `F:\runo` 之下,path 相依最為簡便:

```toml
[dependencies]
open-cpu = { path = "../open-cpu" }
```

若要從工作空間內的成員 crate 使用,建議在工作空間根目錄的
`Cargo.toml` 中寫

```toml
[workspace.dependencies]
open-cpu = { path = "../open-cpu" }
```

並在成員側寫成 `open-cpu = { workspace = true }`。

將來若要改為 git 相依:

```toml
open-cpu = { git = "https://github.com/aon-co-jp/open-cpu", branch = "main" }
```

crate 名稱帶連字號,為 `open-cpu`;從 Rust 參照時的路徑帶底線,
為 `open_cpu`。

## 2. 替換 CPU 功能偵測程式碼

移轉前(各儲存庫中常見的樣式):

```rust
static HAS_AVX2: OnceLock<bool> = OnceLock::new();
fn has_avx2() -> bool {
    *HAS_AVX2.get_or_init(|| is_x86_feature_detected!("avx2"))
}
```

移轉後:

```rust
if open_cpu::detect().avx2 { /* ... */ }
```

`detect()` 內部已使用 `OnceLock` 快取,因此呼叫端不需要再另外快取。
由於回傳的是 `&'static CpuCapabilities`,也不會發生配置記憶體的情形。

可用的欄位: `avx2` `avx512f` `avx512bw` `avx512vl` `pclmulqdq`
`bmi1` `bmi2` `fma` `aes` `popcnt` `sha` `sse2` `ssse3` `avx_vnni` `avx512vnni`。

## 3. 替換 GF(2^8) / 同位運算(主要針對 `open-raid-z`)

`open-cpu` 的不可約多項式為 `0x11d`,生成元為 `g = 2`,與 Linux md/RAID6 以及
ZFS RAID-Z 相同。**移轉前務必確認自家儲存庫的多項式與生成元是否與此一致。**
若不同,數值就會對不起來。

| 移轉前常見的形式 | 移轉後 |
|---|---|
| `for i in .. { p[i] ^= d[i] }` | `open_cpu::gf_xor(&mut p, &d)` |
| `for i in .. { q[i] ^= gf_mul(d[i], c) }` | `open_cpu::gf_mul_parity(&mut q, &d, c)` |
| `for i in .. { acc[i] = mul2(acc[i]) ^ d[i] }` | `open_cpu::gf_mul2_xor(&mut acc, &d)` |
| `for i in .. { acc[i] = mul4(acc[i]) ^ d[i] }` | `open_cpu::gf_mul4_xor(&mut acc, &d)` |
| 任意次數的 `×2^n` 霍納法 | `open_cpu::gf_mul_pow2_xor(&mut acc, &d, n)` |
| 自有的 `gf_mul(a: u8, b: u8) -> u8` | `open_cpu::gf_mul(a, b)`(`const fn`) |
| 自有的 `mul2_byte(b) -> u8` | `open_cpu::gf_mul2_byte(b)`(`const fn`) |
| 自有的 `g^i` 係數計算 | `open_cpu::raid6_coeff(i)` |
| P/Q 一次計算 | `open_cpu::raid6_parity(&stripes, &mut p, &mut q)` |
| P/Q/R 一次計算(相當於 RAID-Z3) | `open_cpu::raid6_parity3(&stripes, &mut p, &mut q, &mut r)` |

`gf_mul_parity` / `gf_xor` 在 `dst.len() != src.len()` 時會 panic。
請在呼叫端先把分條長度對齊。

若想明確呼叫特定的實作(供效能量測或驗證用途):

- `open_cpu::gf_mul_parity_scalar(...)` / `gf_xor_scalar(...)` /
  `gf_mul_pow2_xor_scalar(...)` — safe
- `unsafe { open_cpu::gf_mul_parity_avx2(...) }` — 由呼叫端保證支援 AVX2
- `unsafe { open_cpu::gf_mul_parity_pclmul(...) }` — 同理,需 SSSE3+PCLMULQDQ
- `unsafe { open_cpu::gf_mul_parity_avx512(...) }` — **執行未驗證**

## 4. 移轉後的驗證(必須)

1. `cargo build` — 相依解析與建置能夠通過。
2. `cargo test` — **既有測試全數通過**。特別要確認替換前後
   同位資料的位元組序列完全一致。若既有測試中沒有
   同位值的比對,應在移轉時補上。
3. 若在記錄中輸出一行 `open_cpu::runtime_summary()`,
   日後就能確認在實機上選用了哪一個實作。

## 4.5 實際的移轉範例(`open-raid-z`,2026-08-22)

作為參考,列出首次移轉實例的要點:

- 把 `detect_level()` 內的 `std::is_x86_feature_detected!` 替換為
  對 `open_cpu::detect()` 的參照(`SimdLevel` 這個
  儲存庫特有的列舉型別原樣保留,**只把其判定依據**
  移交給 open-cpu)。既有的呼叫端完全不需要更動。
- 只把 `gf_mul_xor_into()` / `xor_into()` / `mul_pow2_xor_into()` 的
  **AVX2 路徑**委由 open-cpu 處理,而 AVX-512 路徑(open-cpu 側同樣
  未驗證)與 SSE2 路徑(open-cpu 中沒有實作)則保留了儲存庫側的實作。
  這是基於**不做會使效能下降的替換**的判斷。
- 不再被使用的 SIMD kernel 並未刪除,而是加上 `#[allow(dead_code)]`
  保留下來,作為將來與 open-cpu 側交叉驗證時的參照。

建議採取這種「不一次全部替換,而是從等價且效能不會下降的部分開始逐步
委由處理」的推進方式。

## 5. 注意事項

- **AVX-512 路徑執行未驗證**(開發機未搭載)。由於預設不會被選用,
  移轉並不會使行為產生變化。只有要在 AVX-512 機器上驗證時,才
  設定 `OPEN_CPU_ENABLE_AVX512=1`。
- POPCNT/BMI/FMA/AES-NI/SHA-NI/VNNI **僅有偵測**。使用它們的
  檢查碼、壓縮、矩陣運算的實作在 `open-cpu` 中尚不存在,因此
  `aruaru-db` / `aruaru-llm` 側的相關程式碼還無法移轉
  (只先移轉偵測部分則是可行的)。
- 在 x86 以外(ARM 等)所有功能都會是 `false`,並落到純量實作。
  交叉建置可以通過,但尚未支援 NEON 等最佳化。
