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

- **2026-08-23 複数命令セットの「組み合わせ」ディスパッチ + FMA3/POPCNT/BMI
  演算実装 + 技術動向調査**:

  ### 1. 組み合わせディスパッチ API(`src/isa.rs` 新設)
  実在の CPU は AVX2+FMA3、AVX-512F+BW+VNNI のように複数命令セットを
  同時搭載する。従来の bool フィールド羅列では組み合わせ判定を呼び出し側が
  毎回手書きする必要があったため、以下を追加した(既存フィールドは変更
  していないので後方互換)。
  - `Feature`(17 種の列挙型、文字列との相互変換)、`FeatureSet`
    (ビットマスク集合、`contains_all` / `contains_any` / 積和差)。
  - `IsaProfile`(8 段階の組み合わせプロファイル)、`isa_profile()` /
    `at_least()` / `supports_all()`。
  - `select(&[(tag, &[Feature])])`(優先順の候補から成立する最初のものを選ぶ)。
  - `detected_but_unused()`(検出済みだが未活用の機能を返す。「検出だけして
    使っていない」状態を呼び出し側から可視化するため)。
  - **テストで見つけた設計上の誤り**: 当初「上位プロファイルは下位の要求を
    必ず包含する」という単調性テストを書いたところ 2 回失敗した。(a) AVX2
    段階に PCLMULQDQ を含めていなかった、(b) **AVX-512 VNNI 搭載 CPU が
    AVX-VNNI(256bit 版)を報告するとは限らない**ため主系列に置けなかった。
    後者は `Avx2Vnni` を主系列からの「分岐」として扱う設計へ修正した。
    机上で組み合わせを列挙しただけでは気付かなかった点。

  ### 2. FMA3 / POPCNT / BMI1 / BMI2 の演算実装(`src/math.rs` 新設)
  「検出フィールドだけ」だった命令に実際の演算を付けた。
  `dot_f32` / `axpy_f32` / `scale_f32` / `popcount_bytes` /
  `hamming_distance` / `extract_bits` / `deposit_bits` / `trailing_zeros_u64`。
  全てスカラー参照実装との一致を端数長込みでテスト。**28 テスト +
  doctest 5 件通過、警告ゼロ**。実測(Ryzen 9 3950X):
  - `dot_f32`: **3.17x**(4.23 → 13.40 GFLOP/s)
  - `popcount_bytes`: **30.9x**、`hamming_distance`: **11.0x**
  - `axpy_f32`: **1.03x**(メモリ帯域律速。速くなっていないと正直に記録)

  ### 3. ⚠️ 最重要: BMI2(pext/pdep)は機能ビットだけで判断してはいけない
  調査で「Zen〜Zen 2 では pext/pdep がマイクロコード実装で遅い」と分かった
  ため実測したところ、**この開発機(Ryzen 9 3950X = Zen 2, family 17h)では
  BMI2 の pext がスカラーより 7.1 倍遅い**ことを確認した
  (scalar 177ms vs bmi2 1269ms、`examples/bench.rs` の `bench_pext`)。
  素朴に「BMI2 があるから使う」と書いていたら 7 倍の性能退行だった。
  対策として CPUID のベンダ+family を読む `vendor_family()` と
  `CpuCapabilities::fast_bmi2()` を追加し、AMD family 17h 以下では
  スカラーを選ぶようにした。AMD の最適化ガイド(family 19h = Zen 3)は
  「ALU でネイティブ実行、1/cycle・3 cycle レイテンシ。高速/低速の経路を
  持つソフトウェアは family 19h では高速側を選ぶこと」と明記している。
  **教訓: 機能ビットの有無 ≠ その命令が速い。必ず実測する。**

  ### 4. 技術動向調査(一次資料での裏取り)
  ユーザー指示により日英中韓で最新動向を調査した。結論と出典:

  **RAID6 GF(2^8) / CRC —— アルゴリズムは枯れているが GFNI 移行が進行中**
  - Intel ISA-L 2.32 が **AVX2+GFNI / AVX512+GFNI の `pq_gen`** と
    **AVX2+VCLMUL の CRC64/32/16** を追加。つまり GF(2^8) 乗算は
    従来の split-table `pshufb` から **GFNI(`vgf2p8mulb` /
    `vgf2p8affineqb`)** へ、CRC は PCLMULQDQ から **VPCLMULQDQ**
    (256/512bit)へ移りつつある。
    https://github.com/intel/isa-l/blob/master/Release_notes.txt
    https://github.com/intel/isa-l/tree/master/erasure_code
  - Linux カーネルの GF syndrome(`lib/raid6/avx512.c`)は 2016 年から
    ほぼ不変だが、隣接する XOR パリティ経路
    (`xor_gen()`)は 2026 年に Eric Biggers が AVX-512 実装を投稿し
    最大 43% 改善を報告(**mainline へマージ済みかは未確認**)。
    https://www.phoronix.com/news/AVX-512-Xor-Gen-More-Perf
    https://github.com/torvalds/linux/blob/master/lib/raid6/avx512.c
  - OpenZFS の `vdev_raidz_math.c` は sse2/ssse3/avx2/avx512f/avx512bw
    のままで **GFNI 実装は master に無い**。ただし
    「起動時に全実装をベンチマークして最速を選ぶ」という設計は現役で、
    機能ビットだけでは分からない実効性能(AVX-512 のダウンクロック等)を
    扱う手法として参考になる。
    https://github.com/openzfs/zfs/blob/master/module/zfs/vdev_raidz_math.c
  - → **判定: 概ね枯れているが GFNI/VPCLMULQDQ という明確な次の一手がある。**
    本クレートでは GFNI / VPCLMULQDQ の**検出フィールドを追加**したが、
    開発機(Zen 2)が非搭載で実機検証できないため演算実装は見送った。

  **AI 推論(FMA3 / VNNI / AMX)—— 活発に発展中**
  - llama.cpp のディスパッチは関数単位ではなく **バイナリ単位**。
    `GGML_CPU_ALL_VARIANTS=ON` で `libggml-cpu-haswell` /
    `-skylakex` / `-icelake` / `-alderlake` / `-sapphirerapids` 等を
    別々の共有ライブラリとしてビルドし、ロード時に host が対応する
    最良のものを選ぶ。各 variant は `avx512_vnni` + `amx_int8` + `bmi2` の
    ような **機能の束(named tier)** として定義されており、
    組み合わせを直積で扱ってはいない。
    https://github.com/ggml-org/llama.cpp/blob/master/ggml/src/ggml-cpu/CMakeLists.txt
  - `Q4_0_4_4` / `Q4_0_4_8` 等の**専用ファイル形式は削除**され、
    プレーンな `Q4_0` を読み込み時に host 最適レイアウトへ
    **online repack** する方式へ移行した(PR #9921 / #10446)。
    しかも 2025〜2026 も repack 起因のクラッシュ修正が続いており、
    枯れていない。
    https://github.com/ggml-org/llama.cpp/pull/10446
    https://github.com/ggml-org/llama.cpp/issues/10757
  - oneDNN は Xbyak による **実行時 JIT** + ISA の全順序ラダー
    (`avx2 < avx512_core < avx512_core_vnni < ... < avx10_1_512_amx_fp16`)。
    https://uxlfoundation.github.io/oneDNN/dev_guide_cpu_dispatcher_control.html
  - → **判定: 活発に発展中。** ただし AMX / repack は本エコシステムの
    規模に対して過剰であり、今回は取り込まない。**named tier で
    ディスパッチする**という設計思想(llama.cpp の variant)は
    `IsaProfile` として今回取り入れた。

  **BMI1/BMI2 —— 枯れており、かつ用途が狭い**
  - 主な実用例は今もチェス AI の PEXT bitboard。
    https://www.chessprogramming.org/BMI2
  - Zen 5 では pext/pdep が 3/cycle まで改善(x86 P コアで初めて 1/cycle 超)。
    https://www.numberworld.org/blogs/2024_8_7_zen5_avx512_teardown/
  - 遅い CPU 向けの移植可能な代替として CLMUL で pext/pdep を
    エミュレートする **ZP7** がある。https://github.com/zwegner/zp7
  - `tzcnt` / `lzcnt` / `popcnt` は全 CPU で高速でディスパッチ不要。
  - → **判定: 枯れている。** 本クレートは `fast_bmi2()` 判定で対応した
    (ZP7 の導入は、pext がボトルネックになる用途が実際に出てきてから)。

  **Rust のランタイム多機能ディスパッチ**
  - **`target_feature_11` が Rust 1.86(2025-04-03)で安定化**し、
    `#[target_feature]` を **safe fn** に付けられるようになった。
    上位機能を持つ関数から下位を呼ぶのは safe。これによりカーネル本体の
    `unsafe` を廃し、ディスパッチ境界の 1 箇所だけに `unsafe` を
    集約できる。https://github.com/rust-lang/rust/issues/136058
  - `multiversion` 0.8.0(2026-07-24)が事実上の標準クレートで、
    `"x86_64+avx+avx2+fma"` や `x86-64-v3` 相当のターゲット指定と
    indirect / direct ディスパッチ戦略を持つ。
    https://docs.rs/multiversion/latest/multiversion/
  - → **判定: 1.86 が転換点。** ただし本クレートは
    「依存クレートを増やさない」方針のため `multiversion` は採用せず、
    `target_feature_11` によるリファクタは次回以降の課題とする。

  ### 5. 今回の統合先(全て `cargo test` 実機通過)
  - `open-cuda`: `opencuda-blas` の独自 `is_x86_feature_detected!` を全廃し
    open-cpu へ移譲。**さらに実バグを 2 件修正**——(a) AVX-512 経路が
    opt-in 無しで選ばれる状態だった(実機未検証コードが AVX-512 機で
    自動的に走ってしまう)、(b) int8 VNNI の分岐が `f.avx512vnni` 単独
    フラグで、`target_feature` に列挙した `avx512bw` / `avx512f` を
    確認していなかった。組み合わせ判定へ修正。34 テスト通過。
  - `aruaru-llm`: `GET /v1/runtime` に `cpu_simd` を追加。実機起動して
    curl で確認済み(`isa_profile: "avx2+fma3"`, `vnni_path: false`)。
  - `open-english`: `GET /v1/cpu-runtime` を組み合わせ情報付きへ拡張。
    実機起動して curl で確認済み。**本体に CPU 集約処理が無いため
    引き続き表示専用**と正直に記録した。
  - `open-cg-cad`: `tunnel_noise::micro_pressure_wave_index` が
    非ゼロ 2 個/行の疎な差分行列を**密行列として実体化**して GEMM して
    いた(n=2000 で 16 MiB / O(n^2))。`axpy_f32` 2 回の O(n) 実装へ
    置換し、旧実装を `micro_pressure_wave_index_gemm` として残して
    一致テストを追加。**実測 n=2000 で 105.6 倍**。101 テスト通過。
  - `open-raid-z`: 既存統合に回帰なし(47 テスト通過)。

- **次にすべきこと**:
  1. **GFNI による GF(2^8) 乗算**(ISA-L 2.32 相当)。今回検出のみ追加した。
     `vgf2p8mulb` は 1 命令で 16/32/64 バイト分の GF 乗算ができ、
     split-table `pshufb` より原理的に速い。**ただし開発機が非搭載**
     なので、GFNI 搭載機(Intel Ice Lake 以降 / AMD Zen 5 以降)を
     確保できたときに実装+実測すること。
  2. **VPCLMULQDQ による CRC**。`aruaru-db` のチェックサムが実需になった
     ときに。同じく開発機は非搭載。
  3. **`target_feature_11`(Rust 1.86+)へのリファクタ**。カーネルを
     safe fn 化し、`unsafe` をディスパッチ境界 1 箇所へ集約する。
  4. AVX-512 搭載機での実測検証(`OPEN_CPU_ENABLE_AVX512=1` 制限の解除)。
     AVX-512 は無条件に速いわけではなく、Skylake-SP 世代では
     ダウンクロックする。OpenZFS 方式の起動時ベンチ選択も検討に値する。
  5. `aruaru-db`(チェックサム・圧縮)への統合は未着手。
  6. SSE2 のみの CPU 向け GF 実装が無く、`open-raid-z` は SSE2 経路だけ
     自前実装を残している(前回からの継続課題)。
  7. `gf_xor` の非一時ストア(`_mm256_stream_si256`)最適化(未検証、
     前回からの継続課題)。

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

## HANDOFF追記(2026-08-23、AI推論での実利用状況を確認 — コード変更なし)

`aruaru-llm`の階層的アクセラレーション作業(CUDA → Vulkan → DirectX →
**CPU SIMD**)にあたり、「open-cpuは検出だけで実際の計算に使われていない
のではないか」という疑いを実コードで検証した。

- **結論: 実際に使われている。** `opencuda-blas::simd`が
  `open_cpu::detect()`を唯一の情報源としており(2026-08-23の一元化作業)、
  `launch_naive_gemm`はCPUデバイスの場合`simd::sgemm_cpu`(AVX-512 →
  AVX2+FMA3 → スカラーの多段ディスパッチ)へ分岐する。
  `open-cuda-llm::Linear::forward`・Attention・MLAはすべてこの
  `opencuda-blas`経由なので、**GPUへオフロードされない計算はすべて
  open-cpuの判定に従ってSIMD実行されている**。追加の配線は不要だった。
- この開発機(Ryzen 9 3950X / Zen 2)の判定は`avx2+fma3+sse2`
  (`isa_profile = avx2+fma3`)。`GET /v1/runtime`(aruaru-llm)・
  `GET /v1/cpu-runtime`(open-english)の両方で実際にこの値が
  報告されることをHTTPで確認した。
- **参考(このリポジトリの価値を裏付ける実測)**: 同日、`aruaru-llm`で
  D3D12 GPU(GT 730)へGEMMをオフロードする経路を実装して実測したところ、
  **AVX2のCPU GEMMの方が3〜30倍速かった**。安価・低性能なGPUしか無い
  環境では、GPUへ逃がすより**CPU SIMDを詰める方が効く**という
  過去HANDOFF(2026-08-22、CPU SIMD化で実測3.34倍)の判断が改めて
  裏付けられた形になる。詳細は`open-cuda/CLAUDE.md`の同日エントリ参照。
- **このリポジトリのコードは変更していない**(確認のみ)。

- 次にすべきこと: 変更なし(AVX-512/VNNI経路が実機未検証である点を含め、
  既存のHANDOFFの課題がそのまま残る)。

