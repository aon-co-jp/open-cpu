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

## 多言語ドキュメント

`README/` フォルダに 15 言語版の README / CLAUDE / PORTING を配置している
(エコシステム共通の運用、`open-raid-z` / `open-cuda` と同じ命名規則)。
**日本語版(リポジトリ直下の `README.md` / `CLAUDE.md` / `PORTING.md`)が
正本**であり、内容を更新した際は 15 言語版も追従させること
(自動同期の仕組みは無い、手動で反映する)。

言語: US English / UK English / Germany / Italy / France / Spain / Russia /
Ukraine / Hebrew / Persian(Iran) / Arabic / China / Taiwan / Korea / Japan。

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
    → **この avx2 の倍率は後日の再計測で 11.6〜18.1 倍の範囲に変動すると
    判明した**(SIMD 側がメモリ帯域律速のため。最新の数値と範囲は
    README.md の「実測ベンチマーク」節が正)。
  - `cargo test --release` 全 11 テスト + doctest 1 件通過。PCLMULQDQ 実装は
    全 256 係数でスカラーと一致することを確認済み。
  - 統合実績: `open-raid-z`(GF 演算の置き換え)と `open-english/server`
    (`/v1/cpu-runtime` エンドポイントと起動ログ)へ path 依存で組み込み済み。

- **2026-08-22(続き)実用性向上サイクル + 自己紹介機能**:
  初期実装の後、連携性・実用性を高めるための開発→TEST→修正サイクルを実施した。
  - **サイクル1: ホーナー法 API の追加**(`gf_mul_pow2_xor` /
    `gf_mul2_xor` / `gf_mul4_xor`、AVX2 + スカラー)。これにより
    `open-raid-z` の `mul2_xor_into` / `mul4_xor_into` の AVX2 経路も
    open-cpu へ移譲でき、あちら側の x86 コードをさらに削減できた。
    `gf_xor` にも AVX2 経路を追加(移譲が性能退行にならないようにするため)。
  - **サイクル2: 使い勝手の改善**。`CpuCapabilities` に `Display` 実装と
    `has_all()` を追加。RAID-Z3 相当の P/Q/R を一括計算する
    `raid6_parity3()` を追加。`examples/bench.rs` に XOR・ホーナー法の
    計測を追加。
  - **検証**: `cargo test --release` **全 15 テスト + doctest 2 件通過**。
    ベンチは 4 回連続実行し、ばらつき(SIMD 側がメモリ帯域律速のため
    11.6〜18.1 倍と変動)を README に正直に範囲で記録した。
  - **`open-english` への統合で見つけた実バグ(重要な教訓)**: 「誰が
    作ったのか」への自己紹介応答機能をキーワード部分一致
    (`"誰が作"`)で実装したところ、**実ブラウザでのテストで
    「誰が【このシステムを】作ったのですか?」が検出できない**ことが
    判明した(疑問詞と動詞の間に語句が入るため)。疑問詞リストと動詞
    リストを分けて AND 条件で判定する形へ修正し、肯定 10 例・否定 6 例の
    確認と実ブラウザでの E2E 確認を行った。**「ユニットテストが通った」
    ではなく「実際にチャットへ入力してみる」ところまでやらないと
    見つからない類のバグ**だった、という記録。

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
  5. SSE2 のみの CPU 向け実装が無いため、`open-raid-z` は SSE2 経路だけ
     自前の実装を残している。SSE2 版を追加すればそこも移譲できる。
  6. `gf_xor` は SIMD 化してもスカラー比 1.12〜1.25 倍しか出ない
     (メモリ帯域律速)。非一時ストア(`_mm256_stream_si256`)で
     キャッシュ汚染を避ける最適化が効く可能性があり、大きなバッファ向けに
     試す価値がある(未検証)。
