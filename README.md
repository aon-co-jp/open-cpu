# open-cpu

`aon-co-jp` エコシステム共通の **CPU 命令セット検出・ランタイムディスパッチ
ライブラリ**(Rust)。

`open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` がそれぞれ独自に
CPU 機能検出コードを書いて重複するのを避けるために新設した。

**常駐サービス(デーモン)ではない。** 各リポジトリが `Cargo.toml` の
`[dependencies]` に追加し、同一プロセス内へリンクして使う通常の
ライブラリクレート。

## できること

1. **CPU 機能のランタイム検出** — `open_cpu::detect()` が
   `&'static CpuCapabilities` を返す。`std::is_x86_feature_detected!` を使い、
   初回検出結果を `OnceLock` にキャッシュするので何度呼んでもコストはゼロに近い。
2. **RAID6 GF(2^8) 演算のランタイムディスパッチ** — 検出結果に応じて
   スカラー / PCLMULQDQ / AVX2 / AVX-512 の実装を実行時に選択する。

## 使い方

```toml
# 依存側の Cargo.toml
[dependencies]
open-cpu = { path = "../open-cpu" }
```

```rust
// 1. CPU 機能検出
let caps = open_cpu::detect();
println!("{}", caps.summary());
// => sse2 ssse3 popcnt aes pclmulqdq bmi1 bmi2 fma sha avx2

if caps.avx2 { /* ... */ }

// 2. RAID6 パリティ
let d0 = vec![1u8; 4096];
let d1 = vec![2u8; 4096];
let mut p = vec![0u8; 4096];
let mut q = vec![0u8; 4096];
open_cpu::raid6_parity(&[&d0, &d1], &mut p, &mut q);

// 個別 API
open_cpu::gf_xor(&mut p, &d0);                 // p ^= d0            (P パリティ)
open_cpu::gf_mul_parity(&mut q, &d0, 0x02);    // q ^= d0 * 0x02     (Q パリティ)

// ログ 1 行サマリ
println!("{}", open_cpu::runtime_summary());
// => open-cpu 0.1.0 | features: ... | gf impl: Avx2
```

## 検出対象の命令セット

| 命令セット | 検出 | このクレートでの利用 |
|---|---|---|
| SSE2 | ✅ | PCLMULQDQ パスの補助として使用 |
| SSSE3 | ✅ | `pshufb`(PCLMULQDQ パスの還元) |
| PCLMULQDQ | ✅ | GF(2^8) 乗算(キャリーレス乗算実装) |
| AVX2 | ✅ | GF(2^8) 乗算(`vpshufb` split-table、既定で選択) |
| AVX-512F / BW / VL | ✅ | GF(2^8) 乗算パスあり(**実行未検証**、下記参照) |
| POPCNT | ✅ | 検出のみ(利用実装なし) |
| BMI1 / BMI2 | ✅ | 検出のみ(利用実装なし) |
| FMA3 | ✅ | 検出のみ(利用実装なし) |
| AES-NI | ✅ | 検出のみ(利用実装なし) |
| SHA-NI | ✅ | 検出のみ(利用実装なし) |
| AVX-VNNI | ✅ | 検出のみ(将来の AI 推論向け、利用実装なし) |
| AVX-512 VNNI | ✅ | 検出のみ(将来の AI 推論向け、利用実装なし) |

x86/x86_64 以外のアーキテクチャでは全フィールドが `false` になり、
スカラー実装へフォールバックする(ビルドは通る)。

## GF(2^8) 実装の詳細

既約多項式は `0x11d`(x^8+x^4+x^3+x^2+1)、生成元は `g = 2`。
Linux md/RAID6 および ZFS RAID-Z と同じ。

- **スカラー**: nibble split テーブル(16 エントリ × 2)による参照実装。
- **PCLMULQDQ**: 各バイトを 16bit 間隔に展開すると、8bit 係数とのキャリーレス積
  (最大 15bit)が隣接スロットへ桁上がりしない。この性質を使い 1 命令で 4 バイト
  分をまとめて乗算し、`pshufb` 2 回で GF(2^8) へ還元する。16 byte/iter。
- **AVX2**: `vpshufb` による split-table 実装。32 byte/iter。
- **AVX-512F/BW**: 同 split-table を 64 byte/iter で処理。

## 実測ベンチマーク

`cargo run --release --example bench`(4 MiB × 50 回 = 200 MiB、factor=0x8d)

開発機: **AMD Ryzen 9 3950X** / Windows 11 / rustc 1.96.0

```
open-cpu 0.1.0 | features: sse2 ssse3 popcnt aes pclmulqdq bmi1 bmi2 fma sha avx2 | gf impl: Avx2
scalar   :  199.356 ms   1003.23 MiB/s
pclmulqdq:  104.398 ms   1915.74 MiB/s  (1.91x vs scalar)
avx2     :    8.877 ms  22530.64 MiB/s  (22.46x vs scalar)
avx512   : このCPUでは未搭載のため実測不可(未検証)
```

**AVX2 はスカラー比 22.46 倍(実測)。** PCLMULQDQ は 1.91 倍で、AVX2 が
使えない古い CPU 向けのフォールバックとしてのみ意味がある。

## 検証状況(正直な開示)

- ✅ **スカラー / PCLMULQDQ / AVX2**: 上記開発機で実行検証済み。
  `cargo test` にて、素朴なビットシフト実装を基準にスカラー実装の正しさを、
  さらにスカラー実装を基準に PCLMULQDQ 実装(**全 256 通りの係数 × 9 通りの
  長さ**)と AVX2 実装(8 通りの係数 × 11 通りの長さ、端数処理含む)の
  出力一致を確認している。全 11 テスト + doctest 1 件が通過。
- ⚠️ **AVX-512 パスは実行未検証**。開発機(Ryzen 9 3950X)が AVX-512 非搭載の
  ため、**コンパイルが通ることのみ**確認している。安全のため既定の
  ディスパッチでは選択されず、環境変数 `OPEN_CPU_ENABLE_AVX512=1` を
  設定した場合のみ opt-in で有効になる。AVX-512 搭載機で検証が済むまで
  この扱いを維持する。
- ⚠️ POPCNT/BMI1/BMI2/FMA/AES-NI/SHA-NI/VNNI は**検出フィールドがあるだけ**で、
  これらを使った演算実装はまだ無い。

## テスト・ベンチの実行

```
cargo build --release
cargo test --release
cargo run --release --example bench
```

## 関連

- 移行手順: [PORTING.md](PORTING.md)
- 開発方針・HANDOFF: [CLAUDE.md](CLAUDE.md)
- GitHub organization: https://github.com/aon-co-jp
