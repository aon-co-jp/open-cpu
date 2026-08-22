# 開発方針＆開発環境ルール(open-cpu)

このリポジトリは `aon-co-jp` エコシステムの一員。**全リポジトリ共通の設計思想・
運用ルール(検証徹底・誇張報告の禁止・車輪の再発明回避・確認不要の自動継続等)
は [`open-raid-z/CLAUDE.md`](https://github.com/aon-co-jp/open-raid-z/blob/main/CLAUDE.md)
を正本とする。** ここには複製せず、このリポジトリ固有の事項のみを記載する。

作業ドライブは `F:\runo\open-cpu`(新レイアウト)。

## このリポジトリの役割

`open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` が**それぞれ独自に**
CPU 命令セット(AVX2/AVX-512/PCLMULQDQ/BMI1/BMI2/FMA3/AES-NI/POPCNT/SHA-NI)の
ランタイム検出・ディスパッチを実装しようとしていたため、その共通基盤として新設した。

- **ライブラリクレートである**(ユーザー指示、2026-08-22)。常駐型サービス
  (デーモン)にはしない。各リポジトリが `[dependencies]` に追加し、
  同一プロセス内へリンクして使う。プロセス間通信は導入しないこと。
- スコープは「CPU 機能検出」と「その検出結果に基づく演算ディスパッチ」に限る。
  各リポジトリのドメインロジック(RAID のストライピング戦略、DB のページ管理、
  LLM のモデル読み込み等)をここへ移植しないこと。
- 依存クレートを増やさない(現状 `[dependencies]` は空、`std` のみ)。
  各リポジトリの最下層に入る基盤なので、依存を持ち込むと全体へ波及する。

## 実装・検証ルール(このリポジトリ固有)

- **SIMD 実装を追加したら、必ずスカラー実装との出力一致テストを同時に書く。**
  端数(ベクタ幅の倍数でない長さ)を必ずテストケースに含めること。
- **実機で実行できないコードパスは「未検証」と明記し、既定のディスパッチで
  選択させない。** 現在 AVX-512 パスがこれに該当する(開発機の
  AMD Ryzen 9 3950X が AVX-512 非搭載)。`OPEN_CPU_ENABLE_AVX512=1` による
  opt-in のみで有効になる。AVX-512 搭載機で検証が済んだ時点でこの制限を外し、
  README.md / PORTING.md / 本ファイルの「未検証」表記を更新すること。
- ベンチマークの数値を書くときは、**実測したものだけ**を「実測」と書く。
  推定値・理論値を実測のように書かない。
- `cargo bench` は使わず、`examples/bench.rs` の `std::time::Instant` による
  簡易計測で足りる(依存を増やさないため)。

## HANDOFF

- **2026-08-22 新規作成 + 2 リポジトリへの統合**:
  空リポジトリの状態から初期実装を作成した。
  - `src/caps.rs`: `CpuCapabilities` 構造体と `detect()`(`OnceLock` キャッシュ)。
    検出対象は avx2 / avx512f / avx512bw / avx512vl / pclmulqdq / bmi1 / bmi2 /
    fma / aes / popcnt / sha / sse2 / ssse3 / avx-vnni / avx512vnni。
  - `src/gf.rs`: RAID6 の GF(2^8) 演算(多項式 0x11d、生成元 g=2)。
    `gf_xor` / `gf_mul_parity` / `raid6_parity` / `raid6_coeff` を公開し、
    スカラー / PCLMULQDQ / AVX2 / AVX-512 の 4 実装を実行時ディスパッチ。
  - 実測(Ryzen 9 3950X、4MiB×50回): scalar 1003 MiB/s、pclmulqdq 1916 MiB/s
    (1.91x)、**avx2 22531 MiB/s(22.46x)**。AVX-512 は非搭載のため実測不可。
  - `cargo test --release` 全 11 テスト + doctest 1 件通過。PCLMULQDQ 実装は
    全 256 係数でスカラーと一致することを確認済み。
  - 統合実績: `open-raid-z`(GF 演算の置き換え)と `open-english/server`
    (`/v1/cpu-runtime` エンドポイントと起動ログ)へ path 依存で組み込み済み。

- **次にすべきこと**:
  1. `aruaru-db` の チェックサム・圧縮まわり、`aruaru-llm` の行列演算、
     `open-cuda` の CPU フォールバックへも順次統合する(現状は
     `open-raid-z` と `open-english` の 2 件のみ)。
  2. AVX-512 搭載機を入手・確保できたら AVX-512 パスを実測検証し、
     opt-in 制限を外す。
  3. 検出のみで実装が無い命令セット(POPCNT/BMI/FMA/AES-NI/SHA-NI)について、
     依存側で実際に必要になったものから演算実装を追加する。特に
     `aruaru-db` のチェックサムには PCLMULQDQ による CRC32C、
     `aruaru-llm` には AVX-VNNI/AVX-512 VNNI による int8 内積が候補。
  4. path 依存(`path = "../open-cpu"`)で組んでいるため、VPS 等
     `F:\runo` レイアウトが無い環境でビルドする場合は git 依存への
     切り替えが必要になる。実際に VPS でビルドする段になったら対応する。
