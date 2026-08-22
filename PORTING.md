# PORTING.md — 他リポジトリから `open-cpu` へ乗り換える手順

`open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` などが個別に持っている
CPU 機能検出コード・GF(2^8) 演算コードを `open-cpu` へ集約するための移行手順。

## 0. 前提

`open-cpu` は **ライブラリクレート**であり、常駐サービスではない。
プロセス間通信や別プロセスの起動は不要で、`Cargo.toml` に依存を足して
関数を呼ぶだけで完結する。

## 1. 依存の追加

ローカル作業ドライブ `F:\runo` 配下では path 依存が最も簡単:

```toml
[dependencies]
open-cpu = { path = "../open-cpu" }
```

ワークスペース内のメンバクレートから使う場合は、ワークスペース root の
`Cargo.toml` に

```toml
[workspace.dependencies]
open-cpu = { path = "../open-cpu" }
```

と書き、メンバ側で `open-cpu = { workspace = true }` とするのが望ましい。

将来 git 依存へ切り替える場合:

```toml
open-cpu = { git = "https://github.com/aon-co-jp/open-cpu", branch = "main" }
```

クレート名はハイフン付き `open-cpu`、Rust から参照する際のパスは
アンダースコア付き `open_cpu`。

## 2. CPU 機能検出コードの置き換え

移行前(各リポジトリによくあるパターン):

```rust
static HAS_AVX2: OnceLock<bool> = OnceLock::new();
fn has_avx2() -> bool {
    *HAS_AVX2.get_or_init(|| is_x86_feature_detected!("avx2"))
}
```

移行後:

```rust
if open_cpu::detect().avx2 { /* ... */ }
```

`detect()` は内部で `OnceLock` キャッシュ済みなので、呼び出し側で
さらにキャッシュする必要はない。`&'static CpuCapabilities` を返すため
アロケーションも発生しない。

利用可能なフィールド: `avx2` `avx512f` `avx512bw` `avx512vl` `pclmulqdq`
`bmi1` `bmi2` `fma` `aes` `popcnt` `sha` `sse2` `ssse3` `avx_vnni` `avx512vnni`。

## 3. GF(2^8) / パリティ演算の置き換え(主に `open-raid-z`)

`open-cpu` の既約多項式は `0x11d`、生成元は `g = 2` で、Linux md/RAID6 および
ZFS RAID-Z と同一。**移行前に、自リポジトリの多項式・生成元がこれと一致するか
必ず確認すること。** 異なる場合は数値が合わなくなる。

| 移行前によくある形 | 移行後 |
|---|---|
| `for i in .. { p[i] ^= d[i] }` | `open_cpu::gf_xor(&mut p, &d)` |
| `for i in .. { q[i] ^= gf_mul(d[i], c) }` | `open_cpu::gf_mul_parity(&mut q, &d, c)` |
| `for i in .. { acc[i] = mul2(acc[i]) ^ d[i] }` | `open_cpu::gf_mul2_xor(&mut acc, &d)` |
| `for i in .. { acc[i] = mul4(acc[i]) ^ d[i] }` | `open_cpu::gf_mul4_xor(&mut acc, &d)` |
| 任意回数の `×2^n` ホーナー法 | `open_cpu::gf_mul_pow2_xor(&mut acc, &d, n)` |
| 独自 `gf_mul(a: u8, b: u8) -> u8` | `open_cpu::gf_mul(a, b)`(`const fn`) |
| 独自 `mul2_byte(b) -> u8` | `open_cpu::gf_mul2_byte(b)`(`const fn`) |
| 独自の `g^i` 係数計算 | `open_cpu::raid6_coeff(i)` |
| P/Q 一括計算 | `open_cpu::raid6_parity(&stripes, &mut p, &mut q)` |
| P/Q/R 一括計算(RAID-Z3 相当) | `open_cpu::raid6_parity3(&stripes, &mut p, &mut q, &mut r)` |

`gf_mul_parity` / `gf_xor` は `dst.len() != src.len()` で panic する。
呼び出し側でストライプ長を揃えておくこと。

特定の実装を明示的に呼びたい場合(ベンチや検証目的):

- `open_cpu::gf_mul_parity_scalar(...)` / `gf_xor_scalar(...)` /
  `gf_mul_pow2_xor_scalar(...)` — safe
- `unsafe { open_cpu::gf_mul_parity_avx2(...) }` — 呼び出し元が AVX2 対応を保証
- `unsafe { open_cpu::gf_mul_parity_pclmul(...) }` — 同 SSSE3+PCLMULQDQ
- `unsafe { open_cpu::gf_mul_parity_avx512(...) }` — **実行未検証**

## 4. 移行後の検証(必須)

1. `cargo build` — 依存解決とビルドが通ること。
2. `cargo test` — **既存テストが全部通ること**。特に、置き換え前後で
   パリティのバイト列が完全一致することを確認する。既存テストに
   パリティ値の比較が無ければ、移行時に追加すること。
3. ログに `open_cpu::runtime_summary()` を 1 行出しておくと、
   実機でどの実装が選択されたか後から確認できる。

## 4.5 実際の移行例(`open-raid-z`、2026-08-22)

参考として、最初の移行実例の要点を挙げる:

- `detect_level()` 内の `std::is_x86_feature_detected!` を
  `open_cpu::detect()` の参照へ置き換えた(`SimdLevel` という
  リポジトリ固有の列挙型はそのまま残し、その**判定材料だけ**を
  open-cpu へ移した)。既存の呼び出し側を一切変更せずに済む。
- `gf_mul_xor_into()` / `xor_into()` / `mul_pow2_xor_into()` の
  **AVX2 経路だけ**を open-cpu へ委譲し、AVX-512 経路(open-cpu 側も
  未検証)と SSE2 経路(open-cpu に実装が無い)はリポジトリ側の実装を
  残した。**性能が下がる置き換えはしない**、という判断。
- 使われなくなった SIMD カーネルは削除せず `#[allow(dead_code)]` を
  付けて残置し、将来 open-cpu 側と相互検証する際の参照とした。

この「全部を一度に置き換えず、等価かつ性能が落ちない部分から段階的に
委譲する」進め方を推奨する。

## 5. 注意点

- **AVX-512 パスは実行未検証**(開発機が非搭載)。既定では選択されないので、
  移行によって挙動が変わることはない。AVX-512 機で検証する場合のみ
  `OPEN_CPU_ENABLE_AVX512=1` を設定する。
- POPCNT/BMI/FMA/AES-NI/SHA-NI/VNNI は**検出のみ**。これらを使った
  チェックサム・圧縮・行列演算の実装は `open-cpu` にまだ無いので、
  `aruaru-db` / `aruaru-llm` 側の該当コードはまだ移行できない
  (検出部分だけを先に移行するのは可能)。
- x86 以外(ARM 等)では全機能が `false` になりスカラー実装へ落ちる。
  クロスビルドは通るが、NEON 等の最適化は未対応。
