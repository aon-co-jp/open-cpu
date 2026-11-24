> Latest translations / 翻訳の最新版: [0-LATEST-TRANSLATIONS.md](0-LATEST-TRANSLATIONS.md)

# open-cpu

> **多言語版 / Other languages**:
> [US English](README/README-US_English.md) ·
> [UK English](README/README-UK_English.md) ·
> [Deutsch](README/README-Germany.md) ·
> [Italiano](README/README-Italy.md) ·
> [Français](README/README-France.md) ·
> [Español](README/README-Spain.md) ·
> [Русский](README/README-Russia.md) ·
> [Українська](README/README-Ukraine.md) ·
> [עברית](README/README-Hebrew.md) ·
> [فارسی](README/README-IRAN_Persian.md) ·
> [العربية](README/README-Arabic.md) ·
> [简体中文](README/README-China.md) ·
> [繁體中文](README/README-Taiwan.md) ·
> [한국어](README/README-Korea.md) ·
> [日本語](README/README-Japan.md)
>
> ※日本語版(このファイル)が正本。各言語版は
> [`CLAUDE.md`](CLAUDE.md) / [`PORTING.md`](PORTING.md) についても
> `README/` フォルダに用意している。

> 📌 **最近の更新(2026-09-13)**: `open-directx`のFFv1レンジコーダー
> 並列化(GPU側でNレーンが並列にテーブル値をlookupする設計、実GT730
> ハードウェアで32〜1536レーン検証済み)とAVX2/AVX-512のgather命令
> (`vpgatherdd`)が技術的に対応する、というアイデアを実際にコードとして
> 実装した(`gather_u8`/`gather_u8_avx2`)。AVX2版はこの開発機
> (AMD Ryzen 9 3950X)で実行検証済み(スカラー参照実装と完全一致)。
> AVX-512版は開発機が非搭載のため未実装。詳細は
> [PORTING.md](PORTING.md)・[CLAUDE.md](CLAUDE.md)参照。
>
> *English*: Implemented, as real code (not just an idea), the
> technical connection between `open-directx`'s FFv1 range-coder
> parallelization (N GPU lanes looking up table values in parallel,
> verified on real GT730 hardware at 32–1536 lanes) and AVX2/AVX-512
> gather instructions (`vpgatherdd`) — `gather_u8`/`gather_u8_avx2`.
> The AVX2 path is executed and verified on this dev machine (AMD
> Ryzen 9 3950X), matching the scalar reference exactly. The AVX-512
> path is unimplemented (this machine lacks AVX-512). See
> [PORTING.md](PORTING.md) / [CLAUDE.md](CLAUDE.md) for details.

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

### 複数命令セットの「組み合わせ」ディスパッチ API(2026-08-23 追加)

実在の CPU は AVX2 と FMA3、AVX-512F と BW と VNNI のように **複数の命令
セットを同時に搭載** している。単独の bool フィールドだけでは
「両方揃っている場合のみこの経路」という判定を呼び出し側が毎回手書き
することになるため、組み合わせを表現する型と関数を追加した。
既存の `caps.avx2` のようなフィールド参照はそのまま動作する(後方互換)。

| API | 内容 |
|---|---|
| `Feature` | 命令セットの列挙型(17 種)。`Feature::from_name("AVX-VNNI")` で文字列からも引ける |
| `FeatureSet` | 命令セットの集合(ビットマスク)。`contains_all` / `contains_any` / 積・和・差集合 |
| `CpuCapabilities::feature_set()` | 検出結果を `FeatureSet` へ畳み込む |
| `CpuCapabilities::supports_all(&[Feature::Avx2, Feature::Fma])` | **組み合わせ判定の中心** |
| `IsaProfile` | 実用上意味のある組み合わせの段階(`baseline` / `sse2` / `ssse3+pclmul` / `avx2` / `avx2+fma3` / `avx2+fma3+vnni` / `avx512f+bw+vl` / `avx512f+bw+vl+vnni`) |
| `CpuCapabilities::isa_profile()` | 成立する最上位プロファイル(AVX-512 は opt-in 時のみ) |
| `CpuCapabilities::at_least(IsaProfile::Avx2Fma)` | 「この組み合わせ以上か」 |
| `CpuCapabilities::detected_but_unused()` | 検出済みだがこのクレートが未活用の機能(正直さの可視化) |
| `select(&[(tag, &[Feature])])` | 候補パスを優先順に並べ、要求機能が全て揃う最初のものを選ぶ |
| `vendor_family()` | CPUID によるベンダ・family(BMI2 の速度判定に使用) |
| `CpuCapabilities::fast_bmi2()` | pext/pdep が **ハードウェア実装で速い** CPU か(下記の重要な注意を参照) |
| `isa_summary()` / `runtime_report()` | 上記をまとめた文字列(ログ・API 表示用) |

```rust
use open_cpu::{Feature, IsaProfile};
let caps = open_cpu::detect();

// 「AVX2 と FMA3 が両方揃っている場合のみ」の判定
if caps.supports_all(&[Feature::Avx2, Feature::Fma]) { /* vfmadd 経路 */ }

// 段階での判定
if caps.at_least(IsaProfile::Avx512) { /* 512bit 幅の経路 */ }

// 優先順の候補から選ぶ
let path = open_cpu::select(&[
    ("avx512", &[Feature::Avx512f, Feature::Avx512bw][..]),
    ("avx2+fma", &[Feature::Avx2, Feature::Fma][..]),
    ("sse2",     &[Feature::Sse2][..]),
]);
```

### 数値・ビット演算 API(2026-08-23 追加)

「検出しているだけ」だった FMA3 / POPCNT / BMI1 / BMI2 について、
実際にその命令を使う演算を追加した。すべてスカラー参照実装との
出力一致をテストで確認している(端数長も含む)。

| API | 内容 | ディスパッチ |
|---|---|---|
| `dot_f32(a, b)` | f32 内積 | AVX-512F(opt-in)/ **AVX2+FMA3** / AVX2 / スカラー |
| `axpy_f32(acc, src, scale)` | `acc += scale * src` | 同上 |
| `scale_f32(dst, scale)` | `dst *= scale` | AVX2 / スカラー |
| `popcount_bytes(data)` | 立っているビット数 | POPCNT / スカラー |
| `hamming_distance(a, b)` | 異なるビット数 | POPCNT / スカラー |
| `extract_bits(v, mask)` | BMI2 `pext` 相当 | **`fast_bmi2()` が真のときのみ BMI2** / スカラー |
| `deposit_bits(v, mask)` | BMI2 `pdep` 相当 | 同上 |
| `trailing_zeros_u64(v)` | BMI1 `tzcnt` | BMI1 / スカラー |
| `selected_float_impl()` / `bit_impl_summary()` | 選択された実装の報告 | — |

#### ⚠️ BMI2(pext/pdep)は「機能ビットが立っていれば速い」ではない

AMD の Zen / Zen+ / Zen 2(CPUID family 17h)および Hygon Dhyana では
`pext`/`pdep` がマイクロコード実装で、**スカラーのループより大幅に遅い**。
AMD の最適化ガイド(family 19h = Zen 3)では ALU でのネイティブ実行
(スループット 1/cycle、レイテンシ 3 cycle)になったと明記されており、
「高速/低速の経路を持つソフトウェアは family 19h では高速側を選ぶこと」
と指示されている。

本クレートはこれを `fast_bmi2()`(CPUID のベンダ+family 判定)で扱い、
遅い CPU ではスカラーを選ぶ。**開発機(Ryzen 9 3950X = Zen 2)での実測**:

```
pext scalar :  177.469 ms
pext bmi2   : 1268.991 ms  (0.14x = スカラーの約 7.1 倍遅い)
```

つまり「BMI2 が使えるから使う」という素朴な実装は、この CPU では
**7 倍の性能退行**になる。`cargo run --release --example bench` で再現できる。

## 検出対象の命令セット

| 命令セット | 検出 | このクレートでの利用 |
|---|---|---|
| SSE2 | ✅ | PCLMULQDQ パスの補助として使用 |
| SSSE3 | ✅ | `pshufb`(PCLMULQDQ パスの還元) |
| PCLMULQDQ | ✅ | GF(2^8) 乗算(キャリーレス乗算実装) |
| AVX2 | ✅ | GF(2^8) 乗算(`vpshufb` split-table、既定で選択) |
| AVX-512F / BW / VL | ✅ | GF(2^8) 乗算パスあり(**実行未検証**、下記参照) |
| POPCNT | ✅ | `popcount_bytes` / `hamming_distance`(実測 31.0x / 11.6x) |
| BMI1 | ✅ | `trailing_zeros_u64`(`tzcnt`) |
| BMI2 | ✅ | `extract_bits` / `deposit_bits`。ただし **Zen〜Zen 2 では遅いため既定でスカラー**(上記参照) |
| FMA3 | ✅ | `dot_f32` / `axpy_f32`(AVX2 と**組み合わせ**て `vfmadd`、内積で実測 3.17x) |
| AES-NI | ✅ | 検出のみ(利用実装なし) |
| SHA-NI | ✅ | 検出のみ(利用実装なし) |
| AVX-VNNI | ✅ | 検出のみ。`IsaProfile::Avx2Vnni` の判定には使用(演算実装なし、開発機に非搭載) |
| AVX-512 VNNI | ✅ | 検出のみ。`IsaProfile::Avx512Vnni` の判定には使用(演算実装なし、開発機に非搭載) |
| GFNI | ✅ | **検出のみ**。GF(2^8) 乗算を 1 命令で行える(Intel ISA-L 2.32 が採用)。開発機が非搭載のため未実装 |
| VPCLMULQDQ | ✅ | **検出のみ**。256/512bit 幅のキャリーレス乗算。同上 |

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

以下は 2026-08-23 に追加した数値・ビット演算カーネルの実測
(Ryzen 9 3950X / Zen 2、`cargo run --release --example bench`)。

```
--- float kernels (impl: avx2+fma3) ---   65536要素 × 2000回
dot scalar     :   61.983 ms    4.23 GFLOP/s
dot dispatch   :   19.565 ms   13.40 GFLOP/s  (3.17x vs scalar)
axpy scalar    :   26.172 ms   10.02 GFLOP/s
axpy dispatch  :   26.331 ms    9.96 GFLOP/s  (0.99〜1.03x、メモリ帯域律速)

--- bit kernels ---                        4 MiB × 50回
popcount scalar  :  290.943 ms     687.42 MiB/s
popcount dispatch:    9.406 ms   21262.80 MiB/s  (30.93x vs scalar)
hamming scalar   :  291.690 ms     685.66 MiB/s
hamming dispatch :   26.596 ms    7519.79 MiB/s  (10.97x vs scalar)

--- pext/pdep (vendor: Amd family: 0x17 | bmi2 bit: true | fast_bmi2(): false) ---
pext scalar :  177.469 ms
pext bmi2   : 1268.991 ms  (0.14x ← BMI2 の方が 7.1 倍遅い)
```

以下は既存の GF(2^8) 演算の実測
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
- ✅ **FMA3 / POPCNT / BMI1 / BMI2**(2026-08-23 追加): 開発機で実行検証済み。
  `dot_f32` / `axpy_f32` / `scale_f32` / `popcount_bytes` / `hamming_distance` /
  `extract_bits` / `deposit_bits` / `trailing_zeros_u64` について、
  スカラー参照実装との出力一致を端数長を含めて確認(全 28 テスト + doctest 5 件通過)。
- ⚠️ **AES-NI / SHA-NI / AVX-VNNI / AVX-512 VNNI / GFNI / VPCLMULQDQ は
  検出フィールドのみ**で、これらを使った演算実装はまだ無い。
  AVX-VNNI / AVX-512 VNNI / GFNI / VPCLMULQDQ はいずれも開発機
  (Zen 2)が非搭載のため、書いても実機検証ができない。
- ℹ️ `axpy_f32` は SIMD 化しても **1.03 倍程度しか速くならない**
  (実測)。読み書き量に対して演算が 2 flop/要素しかなくメモリ帯域律速
  のため。速くなったように書かないための記録。

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
| [`open-english`](https://github.com/aon-co-jp/open-english) | サーバー起動ログの 1 行サマリと、`GET /v1/cpu-runtime`。2026-08-23 に組み合わせプロファイル・`fast_bmi2`・`detected_but_unused` を返すよう拡張。**表示専用**(open-english 本体に CPU 集約的な処理が無いため、高速化の適用先は現時点で無い)。 |
| [`open-cuda`](https://github.com/aon-co-jp/open-cuda) | `opencuda-blas` の `CpuFeatures::detect()` を本クレートへ移譲(独自の `is_x86_feature_detected!` を全廃)。GEMM/内積/int8 VNNI のディスパッチ条件を単独フラグから**組み合わせ判定**へ修正。 |
| [`aruaru-llm`](https://github.com/aon-co-jp/aruaru-llm) | `GET /v1/runtime` に `cpu_simd`(組み合わせプロファイル・選択経路)を追加。CPU 推論の GEMM は `opencuda-blas` 経由で本クレートの検出結果に従う。 |
| [`open-cg-cad`](https://github.com/aon-co-jp/open-cg-cad) | トンネル微気圧波指標の断面積微分を `axpy_f32` 経由の O(n) 実装へ置換(旧 O(n^2) GEMM 比で **n=2000 時 105.6 倍**)。 |

未導入(今後の対象): `aruaru-db`(チェックサム・圧縮)。
`open-fudousan` / `open-koumuten` は調査の結果 CPU 集約的な処理が無く
(在庫数十件の CRUD/Web アプリ)、適用対象外と判断した。

## 関連

- 移行手順: [PORTING.md](PORTING.md)
- 開発方針・HANDOFF: [CLAUDE.md](CLAUDE.md)
- GitHub organization: https://github.com/aon-co-jp

## CPU命令セットの全自動インベントリ(2026-09-21追加) / Automatic CPU feature inventory (added 2026-09-21)

**日本語**: `open_cpu::inventory()`が、x86/x86_64(SSE〜AVX-512各サブセット、AVX-VNNI、GFNI、VAES、BMI、SHA、AES等)とaarch64(NEON・FP16・dotprod・i8mm・bf16・SVE/SVE2・AES/PMULL/SHA・CRC32・LSE等)の命令を自動検出し、Linux/Androidでは`/proc/cpuinfo`から生フラグとコア構成(big.LITTLE、Qualcomm Kryoを含む)も取得する。`cargo run --example inventory`で表示。
検出は「CPUが持つ命令」であり、open-cpuのカーネルが使うか(`used_by_open_cpu`)とは別。ARM向け専用カーネルはまだ無い(検出のみ)。実機: OPPO Reno11 A(Cortex-A55x6+A78x2)、moto g53y(Kryo 4xx Silver x6+Gold x2)で確認済み。

**English**: `open_cpu::inventory()` auto-detects x86/x86_64 (SSE..AVX-512 subsets, AVX-VNNI, GFNI, VAES, BMI, SHA, AES...) and aarch64 (NEON, FP16, dotprod, i8mm, bf16, SVE/SVE2, AES/PMULL/SHA, CRC32, LSE...) features; on Linux/Android it also reads raw flags and the core layout (big.LITTLE, incl. Qualcomm Kryo) from `/proc/cpuinfo`. Run `cargo run --example inventory`.
Detection means "the CPU has the instruction", separate from whether open-cpu kernels use it (`used_by_open_cpu`). No ARM-specific kernels yet. Verified on real devices: OPPO Reno11 A (Cortex-A55x6 + A78x2), moto g53y (Kryo 4xx Silver x6 + Gold x2).

## JSON処理の使用方針: `parse_secure_sonic`(速い型+安全)と`sonic-rs`は使える所では全て使う(2026-10-14、ユーザー指示)
正本は[`RS-JSON`](https://github.com/aon-co-jp/RS-JSON)(`README.md`・`PORTING.md`)。

**方針**: JSONを読む所は、使える所では**全て** `rust-json`(RS-JSON)の `parse_secure_sonic`(`fast`機能、結果は`sonic_rs::Value`)を使う。
`sonic-rs` を直接使える所(型付き構造体への読み込みなど)も同様。使えない所は `parse_secure`(結果は`serde_json::Value`)を使う。
外部から来る信頼できない入力(HTTP本文・ネットワーク越しのファイル・アップロード)は、**必ずサイズ・深さ・重複キーの上限つき**で読む。

**実測(7 MB・4万件)**: `serde_json` 200 ms / `parse_secure` 217 ms / **`parse_secure_sonic` 83 ms(約2.4倍速い、防御込み)** / `sonic-rs`そのまま 33 ms(約6倍、防御なし)。
`serde_json::Value`に読む限りSIMDでもほぼ速くならない(188 ms)。**速さの源は、結果を連続した領域に詰め込む値の型**。

**使える所 / 使えない所**
- ✅ 読み取り中心で、JSONを大きく・頻繁に読む所(キャッシュJSON、設定、APIの応答、ログ、インデックス)。
- ✅ 外部からの入力の入口(速さより防御が目的。`parse_secure`でも可)。
- ⚠️ `serde_json::Value`を関数間で受け渡している既存コードは、型を`sonic_rs::Value`に直す書き換えが要る(`JsonValueTrait`/`JsonContainerTrait`のメソッド)。書き換えずに`serde_json::Value`へ読むと速くならない。
- ❌ WASM(`rust_json::light`、依存ゼロ)、PHP/JavaScript側のJSON処理(Rustの部品は使えない)。
- 注意: 依存先の`sonic-rs`は内部で`unsafe`を使う(自前コードに`unsafe`は無い)。小さなJSON(数KB)では体感差が出ない。速度目的の書き換えは、実測で効く所から。

**導入**: `Cargo.toml`に `rust-json = { path = "../RS-JSON", features = ["fast"] }`。使用例は`RS-JSON/README.md`。

**このリポジトリでの適用状況**: 未適用(方針のみ)。コードの切り替えは、JSONを大きく・頻繁に読む所から実測して順に行う。

## JSON handling policy: use `parse_secure_sonic` (fast type + secure) and `sonic-rs` everywhere they can be used (2026-10-14, user instruction)
The source of truth is [`RS-JSON`](https://github.com/aon-co-jp/RS-JSON) (`README.md`, `PORTING.md`).

**Policy**: wherever JSON is read, use `parse_secure_sonic` from `rust-json` (RS-JSON, the `fast` feature; the result is `sonic_rs::Value`) **everywhere it can be used**.
Use `sonic-rs` directly where possible too (e.g. reading into typed structs). Where it cannot be used, use `parse_secure` (result: `serde_json::Value`).
Untrusted input from outside (HTTP bodies, files over the network, uploads) **must always be read with size / depth / duplicate-key limits**.

**Measured (7 MB, 40,000 records)**: `serde_json` 200 ms / `parse_secure` 217 ms / **`parse_secure_sonic` 83 ms (about 2.4x faster, with the defences)** / raw `sonic-rs` 33 ms (about 6x, no defences).
Reading into `serde_json::Value` gives almost no speed-up even with SIMD (188 ms). **The speed comes from the value type that packs the result into contiguous memory.**

**Where it can / cannot be used**
- ✅ Read-heavy code that parses large or frequent JSON (cache JSON, config, API responses, logs, indexes).
- ✅ Entrances for outside input (the goal there is defence rather than speed; `parse_secure` is fine).
- ⚠️ Existing code that passes `serde_json::Value` between functions needs its type changed to `sonic_rs::Value` (methods from `JsonValueTrait` / `JsonContainerTrait`). Reading into `serde_json::Value` without that change is not faster.
- ❌ WASM (`rust_json::light`, zero dependencies) and the JSON handling on the PHP / JavaScript side (Rust components cannot be used there).
- Note: the `sonic-rs` dependency uses `unsafe` internally (our own code has none). For small JSON (a few KB) the difference is not noticeable; do speed-motivated rewrites where measurements show a benefit.

**Adoption**: in `Cargo.toml`, `rust-json = { path = "../RS-JSON", features = ["fast"] }`. Example in `RS-JSON/README.md`.

**Status in this repository**: Not yet applied (policy only). Code will be switched over place by place, starting where measurements show large or frequent JSON reads.

---

作者 / Author: 石塚正浩（MASAHIRO ISHIZUKA） (https://github.com/aon-co-jp)

## API 方針: REST 全廃・gRPC / Connect へ (2026-11-04)

### 日本語
- **REST は全廃する。新しい API は REST で作らない。** ブラウザや `curl` から呼ぶ API は **Connect の単純呼び出し**(`POST /<パッケージ>.<Service>/<Method>`、`content-type: application/json`、本文は JSON)、サービス間は **gRPC / gRPC-web**(RPoem の管理API `open_runo.admin.v1.Admin` と同じ流儀)。ストリーミングは、必要になってから。
- **既存の REST は、触るときに順に移す。** 移したら旧 REST の経路は消す。移行が終わるまで残るが、**新規には追加しない**。
- **JSON は RS-JSON に統一**する(読み込みは `from_str_secure` / `parse_secure` でサイズ・深さ・重複キーの上限つき、書き出しは `rust_json::fast`)。ブラウザの WASM に `prost` は入れない(サイズが増えるため)。
- **例外(自分たちの API ではなく、相手の規格が決めているもの)**: ACME、OAuth/OIDC のコールバック、第三者(GitHub など)からの Webhook、ヘルスチェック、静的ファイル配信、OTLP など。これらは規格どおりの HTTP を受ける。
- 参考実装: `RPoem`(`open_runo.admin.v1.Admin`)、`open-easy-web`(`server/src/connect.rs`、`proto/open_easy_web/admin/v1/admin.proto`)。
- ※ このリポジトリのコードが既に方針どおりかどうかは、各リポジトリの現状による。この節は方針の記録であり、コードの変更を意味しない。

### English
- **REST is being abolished. New APIs are not built as REST.** APIs called from browsers or `curl` use **Connect unary calls** (`POST /<package>.<Service>/<Method>`, `content-type: application/json`, JSON body); service-to-service calls use **gRPC / gRPC-web** (the same style as RPoem's admin API `open_runo.admin.v1.Admin`). Streaming only when it is actually needed.
- **Existing REST endpoints are migrated one by one, when they are touched.** The old REST route is removed once migrated. Until migration is finished they remain, but **no new ones are added**.
- **JSON is unified on RS-JSON**: reading with `from_str_secure` / `parse_secure` (size, depth and duplicate-key limits), writing with `rust_json::fast`. `prost` is not put into browser WASM (it would grow the binary).
- **Exceptions (shapes dictated by someone else's standard, not our own API)**: ACME, OAuth/OIDC callbacks, webhooks from third parties (e.g. GitHub), health checks, static file serving, OTLP, and so on. These accept plain HTTP as the standard requires.
- Reference implementations: `RPoem` (`open_runo.admin.v1.Admin`) and `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Note: whether the code in a given repository already follows this policy depends on that repository. This section records the policy; it does not mean the code has been changed.

### Deutsch (KI-Übersetzung, redaktionell überarbeitet; noch nicht von Muttersprachlern geprüft)
- **REST wird vollständig abgeschafft; neue APIs entstehen nicht mehr als REST.** APIs, die aus dem Browser oder per `curl` aufgerufen werden, verwenden **unäre Connect-Aufrufe** (`POST /<Paket>.<Service>/<Methode>`, `content-type: application/json`, JSON-Body). Zwischen Diensten kommen **gRPC bzw. gRPC-web** zum Einsatz, im selben Stil wie die Admin-API von RPoem (`open_runo.admin.v1.Admin`). Streaming wird erst ergänzt, wenn es tatsächlich gebraucht wird.
- **Bestehende REST-Endpunkte werden schrittweise umgestellt, und zwar immer dann, wenn man ohnehin an ihnen arbeitet.** Sobald ein Endpunkt umgestellt ist, wird die alte REST-Route entfernt. Bis zum Abschluss der Migration bleiben die übrigen bestehen, es werden aber **keine neuen mehr hinzugefügt**.
- **JSON wird einheitlich über RS-JSON verarbeitet**: Gelesen wird mit `from_str_secure` bzw. `parse_secure` (mit Obergrenzen für Größe, Verschachtelungstiefe und doppelte Schlüssel), geschrieben mit `rust_json::fast`. `prost` wird nicht in das Browser-WASM aufgenommen, da es die Datei vergrößern würde.
- **Ausnahmen (Schnittstellen, deren Form ein fremder Standard vorgibt und die nicht zu unserer eigenen API gehören)**: ACME, OAuth/OIDC-Callbacks, Webhooks von Drittanbietern (z. B. GitHub), Health-Checks, die Auslieferung statischer Dateien, OTLP und Ähnliches. Diese nehmen, wie vom jeweiligen Standard verlangt, gewöhnliches HTTP entgegen.
- Referenzimplementierungen: `RPoem` (`open_runo.admin.v1.Admin`) und `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Hinweis: Ob der Code eines einzelnen Repositorys diese Richtlinie bereits erfüllt, hängt vom jeweiligen Repository ab. Dieser Abschnitt dokumentiert lediglich die Richtlinie; er besagt nicht, dass der Code schon angepasst wurde.

### Русский (перевод выполнен с помощью ИИ и отредактирован; носителями языка пока не проверен)
- **От REST мы полностью отказываемся; новые API больше не создаются в стиле REST.** API, к которым обращаются из браузера или через `curl`, используют **унарные вызовы Connect** (`POST /<пакет>.<Service>/<Method>`, `content-type: application/json`, тело запроса — JSON). Для взаимодействия между сервисами применяются **gRPC и gRPC-web** — в том же стиле, что и административный API RPoem (`open_runo.admin.v1.Admin`). Потоковая передача данных добавляется лишь тогда, когда в ней действительно возникает необходимость.
- **Существующие REST-эндпоинты переводятся поэтапно — по мере того, как с ними приходится работать.** После перевода старый REST-маршрут удаляется. Пока миграция не завершена, оставшиеся эндпоинты сохраняются, но **новые не добавляются**.
- **Работа с JSON унифицирована на основе RS-JSON**: чтение — через `from_str_secure` или `parse_secure` (с ограничениями на размер, глубину вложенности и повторяющиеся ключи), запись — через `rust_json::fast`. `prost` в браузерный WASM не включается, поскольку это увеличило бы размер файла.
- **Исключения (интерфейсы, форму которых определяет чужой стандарт, а не наш собственный API)**: ACME, обратные вызовы OAuth/OIDC, вебхуки сторонних сервисов (например, GitHub), проверки работоспособности, раздача статических файлов, OTLP и тому подобное. Такие интерфейсы принимают обычный HTTP, как того требует соответствующий стандарт.
- Эталонные реализации: `RPoem` (`open_runo.admin.v1.Admin`) и `open-easy-web` (`server/src/connect.rs`, `proto/open_easy_web/admin/v1/admin.proto`).
- Примечание: соответствует ли код конкретного репозитория этой политике, зависит от самого репозитория. Данный раздел лишь фиксирует политику и не означает, что код уже изменён.

## API 方針の補足: 用語の整理と使い分け (2026-11-04)

### 日本語
**1つに統一するものではなく、別の層の話です。** 「REST の代わり」になるのは Connect・gRPC・GraphQL だけで、WASM は画面を作る技術です。

| 用語 | 何か | REST の代わりか |
|---|---|---|
| REST | URL と HTTP メソッドで操作を表す API の作り方 | 全廃する側 |
| **Connect** | `POST /サービス名/メソッド名` に JSON を送る形式。`curl` でもブラウザの `fetch` でも呼べる | はい(**採用**) |
| **gRPC** | 同じ「サービス/メソッド」の考え方を、バイナリ(protobuf)と HTTP/2 で行う形式。速く、ストリーミングも得意 | はい(**サービス同士に採用**) |
| gRPC-web | gRPC のブラウザ向けの変形版 | はい(必要なら) |
| GraphQL | 欲しい項目を問い合わせる形式 | 一部だけ(**問い合わせ(読み取り)専用**) |
| WASM | Rust をブラウザの中で動かす仕組み | いいえ(画面を作る技術) |

- **ブラウザの画面や `curl` から呼ぶ API は Connect**(POST、JSON を RS-JSON で読み書き)。ブラウザの WASM は素の gRPC を話せず、Connect なら `fetch` のままで、`prost` を足さずに済む(画面のサイズも増えない)。
- **サービス同士の通信(サーバー間、重い処理、ストリーミング)は gRPC。** 多くは Connect で足りるので、ストリーミングが必要になった所だけ gRPC に上げる。
- **GraphQL は、まとめて問い合わせたいときの読み取り専用。** 書き込み(操作)には使わない。
- **WASM は画面を作る技術として、そのまま使う。** 画面から呼ぶ API が Connect になっただけ。
- 外部の規格が形を決めるもの(ACME、OAuth のコールバック、第三者の Webhook、ヘルスチェック、静的ファイル、OTLP)は、規格どおりの HTTP のまま。
- **迷ったら Connect。**

### English
**These are not alternatives to unify; they sit at different layers.** Only Connect, gRPC and GraphQL can replace REST; WASM is a way to build the UI.

| Term | What it is | Replaces REST? |
|---|---|---|
| REST | API style that expresses operations through URLs and HTTP methods | The thing being abolished |
| **Connect** | `POST /Service/Method` with a JSON body; callable from `curl` and from a browser `fetch` | Yes (**adopted**) |
| **gRPC** | The same service/method idea over binary (protobuf) and HTTP/2; fast, good at streaming | Yes (**for service-to-service**) |
| gRPC-web | A browser-friendly variant of gRPC | Yes (if needed) |
| GraphQL | A query language: ask for exactly the fields you want | Partly (**read-only queries**) |
| WASM | Runs Rust inside the browser | No (a UI technology) |

- **APIs called from browser screens or `curl` use Connect** (POST, JSON read and written with RS-JSON). Browser WASM cannot speak plain gRPC; with Connect it stays a `fetch`, and `prost` is not needed (the binary does not grow).
- **Service-to-service calls (server to server, heavy work, streaming) use gRPC.** Connect is enough in most places; move a call to gRPC only where streaming is actually needed.
- **GraphQL is read-only**, for queries that gather several pieces of data. It is not used for writes (operations).
- **WASM stays as the way to build the UI.** The API it calls simply became Connect.
- Interfaces whose shape an outside standard dictates (ACME, OAuth callbacks, third-party webhooks, health checks, static files, OTLP) stay plain HTTP.
- **When in doubt, use Connect.**

### Deutsch (KI-Übersetzung, sprachlich überarbeitet; noch nicht von Muttersprachlern geprüft)
**Es geht nicht darum, eine einzige Technik auszuwählen – die Begriffe liegen auf unterschiedlichen Ebenen.** Als Ersatz für REST kommen nur Connect, gRPC und GraphQL in Frage; WASM dagegen ist eine Technik, mit der die Benutzeroberfläche gebaut wird.

| Begriff | Was ist das? | Ersatz für REST? |
|---|---|---|
| REST | Ein API-Stil, bei dem Operationen über URLs und HTTP-Methoden ausgedrückt werden | wird abgeschafft |
| **Connect** | `POST /Dienst/Methode` mit JSON im Body; aufrufbar mit `curl` ebenso wie mit `fetch` im Browser | ja (**gewählt**) |
| **gRPC** | Dasselbe Prinzip aus Dienst und Methode, aber über binäres protobuf und HTTP/2; schnell und gut für Streaming geeignet | ja (**zwischen Diensten**) |
| gRPC-web | Eine für den Browser angepasste Variante von gRPC | ja (bei Bedarf) |
| GraphQL | Abfragesprache, mit der man genau die benötigten Felder anfordert | teilweise (**nur für lesende Abfragen**) |
| WASM | Führt Rust-Code direkt im Browser aus | nein (Technik für die Oberfläche) |

- **APIs, die von Browseroberflächen oder per `curl` aufgerufen werden, verwenden Connect** (POST; JSON wird mit RS-JSON gelesen und geschrieben). WASM im Browser kann kein reines gRPC sprechen. Mit Connect bleibt es bei einem gewöhnlichen `fetch`, und `prost` ist nicht nötig – die WASM-Datei wird dadurch nicht größer.
- **Für die Kommunikation zwischen Diensten (Server zu Server, rechenintensive Aufgaben, Streaming) wird gRPC eingesetzt.** In den meisten Fällen reicht Connect aus; auf gRPC wird nur dort umgestellt, wo Streaming tatsächlich gebraucht wird.
- **GraphQL dient ausschließlich lesenden Abfragen**, wenn mehrere Informationen gebündelt abgefragt werden sollen. Für Schreibzugriffe (Operationen) wird es nicht verwendet.
- **WASM bleibt die Technik für die Oberfläche.** Geändert hat sich lediglich, dass die aufgerufene API nun auf Connect beruht.
- Schnittstellen, deren Form durch einen externen Standard festgelegt ist (ACME, OAuth-Callbacks, Webhooks von Drittanbietern, Health-Checks, statische Dateien, OTLP), bleiben gewöhnliches HTTP.
- **Im Zweifel: Connect.**

### Русский (перевод выполнен с помощью ИИ и литературно отредактирован; носителями языка пока не проверен)
**Это не варианты, из которых нужно выбрать один, а технологии разных уровней.** Заменить REST могут только Connect, gRPC и GraphQL; WASM же — это способ создания пользовательского интерфейса.

| Термин | Что это | Заменяет REST? |
|---|---|---|
| REST | Стиль построения API, при котором операции выражаются через URL и HTTP-методы | отменяется |
| **Connect** | `POST /Сервис/Метод` с телом в формате JSON; вызывается как из `curl`, так и из браузерного `fetch` | да (**выбран**) |
| **gRPC** | Тот же принцип «сервис — метод», но поверх двоичного формата protobuf и HTTP/2; работает быстро и хорошо подходит для потоковой передачи | да (**для связи между сервисами**) |
| gRPC-web | Адаптированный для браузера вариант gRPC | да (при необходимости) |
| GraphQL | Язык запросов: клиент запрашивает ровно те поля, которые ему нужны | частично (**только запросы на чтение**) |
| WASM | Позволяет выполнять код на Rust прямо в браузере | нет (технология для интерфейса) |

- **API, которые вызываются из браузерных интерфейсов или через `curl`, строятся на Connect** (POST; JSON читается и записывается через RS-JSON). Браузерный WASM не умеет работать с «чистым» gRPC. С Connect достаточно обычного `fetch`, а `prost` не требуется — размер WASM-файла не увеличивается.
- **Для обмена между сервисами (сервер — сервер, ресурсоёмкие задачи, потоковая передача) применяется gRPC.** В большинстве случаев достаточно Connect; на gRPC переходят только там, где потоковая передача действительно необходима.
- **GraphQL используется только для чтения**, когда нужно получить сразу несколько связанных данных. Для записи (операций) он не применяется.
- **WASM по-прежнему остаётся технологией для интерфейса.** Изменилось лишь то, что вызываемый им API теперь построен на Connect.
- Интерфейсы, форма которых задана внешним стандартом (ACME, обратные вызовы OAuth, вебхуки сторонних сервисов, проверки работоспособности, статические файлы, OTLP), остаются обычным HTTP.
- **Если есть сомнения — выбирайте Connect.**

## 拡張: スマホをアクセラレーターに(`remote/`)
USB/TCP でつないだスマホ(複数台可)へ行列積を振り分ける `open-cpu-remote`。詳細・実測値は [`remote/README.md`](remote/README.md)。
