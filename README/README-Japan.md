> 日本語原文: [README.md](../README.md)

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

### 公開 API 一覧

| API | 内容 | ディスパッチ |
|---|---|---|
| `detect() -> &'static CpuCapabilities` | CPU 機能検出(`OnceLock` キャッシュ) | — |
| `runtime_summary() -> String` | 検出結果+選択実装の 1 行サマリ | — |
| `selected_impl() -> GfImpl` | GF 演算で選択された実装 | — |
| `gf_xor(dst, src)` | `dst ^= src`(P パリティ) | AVX2 / スカラー |
| `gf_mul_parity(dst, src, factor)` | `dst ^= src * factor`(Q パリティ) | AVX-512(opt-in)/ AVX2 / PCLMULQDQ / スカラー |
| `gf_mul_pow2_xor(acc, src, times)` | `acc = acc * 2^times ^ src` | AVX2 / スカラー |
| `gf_mul2_xor` / `gf_mul4_xor` | 上記の `times=1` / `times=2` | 同上 |
| `raid6_parity(stripes, p, q)` | P/Q を係数テーブル方式で一括計算 | 上記に準ずる |
| `raid6_parity3(stripes, p, q, r)` | P/Q/R をホーナー法で一括計算 | 上記に準ずる |
| `gf_mul(a, b) -> u8` | 1 バイト GF 乗算(`const fn`) | — |
| `gf_mul2_byte(b) -> u8` | 1 バイトの GF 上 2 倍(`const fn`) | — |
| `raid6_coeff(i) -> u8` | RAID6 の係数 `g^i`(`g = 2`) | — |

各実装を明示的に呼ぶ `*_scalar` / `*_avx2` / `*_pclmul` / `*_avx512`
版も公開している(ベンチ・相互検証用、SIMD 版は `unsafe`)。

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
scalar   :  207.5 ms     963 MiB/s
pclmulqdq:  109.0 ms    1835 MiB/s  (1.90x vs scalar)
avx2     :   11.4 ms   17473 MiB/s  (18.14x vs scalar)
avx512   : このCPUでは未搭載のため実測不可(未検証)

xor scalar     :  11.5 ms   17451 MiB/s
xor dispatch   :   9.7 ms   20616 MiB/s  (1.15〜1.25x vs scalar)

horner scalar  :  51.5 ms    3880 MiB/s
horner dispatch:  14.8 ms   13521 MiB/s  (2.70〜3.52x vs scalar)
```

**実測値のばらつきについて(正直な記録)**: 4 回連続実行した結果、
GF 乗算の AVX2 倍率は **11.6〜18.1 倍**、ホーナー法は **2.70〜3.52 倍**、
XOR は **1.12〜1.25 倍** の範囲で変動した(スカラー側は 207 ms 前後で安定)。
SIMD 側は 1 万〜2 万 MiB/s に達しており**メモリ帯域律速**になっているため、
キャッシュ状態や他プロセスの影響を受けやすい。上記の表は 1 回の実行結果を
そのまま載せたもので、倍率は範囲で捉えるのが正確。

- **GF(2^8) 任意係数乗算は AVX2 でスカラー比 11.6〜18.1 倍(実測)**。
  RAID6 の Q パリティ・復旧経路で効く。
- **ホーナー法は 2.70〜3.52 倍(実測)**。スカラー版が既に u64 ビット
  トリックで最適化済みのため、GF 乗算ほどの差は出ない。
- **単純 XOR は 1.12〜1.25 倍**。スカラー(u64)の時点でメモリ帯域に
  張り付いているため、SIMD 化の余地が小さい(想定どおり)。
- PCLMULQDQ は 1.90 倍で、**AVX2 が使えない古い CPU 向けのフォールバック**
  としてのみ意味がある。

## 検証状況(正直な開示)

- ✅ **スカラー / PCLMULQDQ / AVX2**: 上記開発機で実行検証済み。
  `cargo test` にて、素朴なビットシフト実装を基準にスカラー実装の正しさを、
  さらにスカラー実装を基準に PCLMULQDQ 実装(**全 256 通りの係数 × 9 通りの
  長さ**)と AVX2 実装(8 通りの係数 × 11 通りの長さ、端数処理含む)の
  出力一致を確認している。全 15 テスト + doctest 2 件が通過。
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

## 導入実績(2026-08-22 時点)

| リポジトリ | 使い方 |
|---|---|
| [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z) | `zfs_accel_hlsl/src/simd.rs` の CPU 機能検出・GF(2^8) 乗算・XOR・ホーナー法(AVX2 経路)を本クレートへ移譲。移行後も既存 39 テストが全通過し、数値は完全に一致。 |
| [`open-english`](https://github.com/aon-co-jp/open-english) | サーバー起動ログの 1 行サマリと、`GET /v1/cpu-runtime`(実行基盤の CPU 命令セットを JSON で返す)。 |

未導入(今後の対象): `aruaru-db`(チェックサム・圧縮)、
`aruaru-llm`(行列演算)、`open-cuda`(GPU 不在時の CPU フォールバック)。

## 関連

- 移行手順: [PORTING.md](PORTING-Japan.md)
- 開発方針・HANDOFF: [CLAUDE.md](CLAUDE-Japan.md)
- GitHub organization: https://github.com/aon-co-jp
