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


## HANDOFF追記(2026-09-13) gather_u8/gather_u8_avx2を追加(open-directxのFFv1レンジコーダー並列化とのCPU-SIMD対応関係を実装)

`open-directx`側でFFv1レンジコーダーの32〜1536レーン並列化(GPU側の
ワークグループ共有メモリ+バリア方式、実GT730ハードウェア検証済み)を
実装した際、ユーザーから「この設計はAVX2/AVX512のgather命令と関連が
あるのでは」という指摘があり、実際にコードとして検証した。新規
`src/gather.rs`: `gather_u8_avx2`(`_mm256_i32gather_epi32`による
8-wideバッチルックアップ、u8テーブルはi32へゼロ拡張してからgather)を
実装し、この開発機(AMD Ryzen 9 3950X)で実行検証(スカラー参照実装と
256/512要素テーブルで完全一致)。`gather_u8`で`detect()`に応じた
実行時ディスパッチも提供。詳細は`PORTING.md`参照。

**正直な開示**: AVX-512版(16-wide)は開発機がAVX-512非搭載のため未実装
(`gf.rs`のAVX-512パスと同じ制約)。FFv1レンジコーダー本体
(`open-directx::range_coder`)をこの関数で実際に置き換える統合作業は
未実施——今回はアイデアの実証コードとして`open-cpu`単体で完結させた。

`cargo test`: 全緑(35件)。`cargo clippy --all-targets -- -D
warnings`: 新規コードはクリーン、既存の無関係な`isa.rs`/`math.rs`の
3件のみ残存(未変更ファイル・既存lintであることを確認済み)。

## HANDOFF追記(2026-09-21、CPU命令インベントリの新設) / HANDOFF addendum (2026-09-21, CPU inventory)

**日本語**: ユーザー指示「スマホのCPUもopen-cpuでAVX2/AVX512を始め全命令を全自動確認」に対応し、`src/inventory.rs`を新設(`inventory()`/`CpuInventory`/`FeatureStatus`/`CoreGroup`、依存無しのJSON出力`to_json()`)。`is_x86_feature_detected!`/`is_aarch64_feature_detected!`(マクロ引数は`literal`ではなく`tt`)+`/proc/cpuinfo`(`CPU implementer`/`CPU part`でコア名を判定)。aruaru-llmの`/v1/runtime`(`cpu_simd`)へ公開済み。
**未対応**: ARM(NEON/dotprod)向けの演算カーネル、SVE検出の実機確認。**次**: aarch64向けdotprodカーネルと、open-cudaの能力交渉層との接続。

**English**: Added `src/inventory.rs` (`inventory()`/`CpuInventory`/`FeatureStatus`/`CoreGroup`, dependency-free `to_json()`). Uses `is_x86_feature_detected!`/`is_aarch64_feature_detected!` (macro args must be `tt`, not `literal`) + `/proc/cpuinfo` (`CPU implementer`/`CPU part` → core names). Exposed via aruaru-llm `/v1/runtime` (`cpu_simd`).
**Not done**: ARM (NEON/dotprod) kernels, SVE on real hardware. **Next**: an aarch64 dotprod kernel and hookup to open-cuda's capability negotiation.

## HANDOFF追記(2026-09-23、NEON実カーネル新設・5ヶ国語) / HANDOFF (NEON kernels) / 手册追记(NEON内核) / 手冊追記(NEON核心) / HANDOFF-Ergänzung (NEON-Kernels)

**日本語**: ユーザー指示「スマホのコア構成やVPSのAVX512フルセットやローカルPCのAVX2やFMA3などの機能をフルで活かせるように改善改良して機能搭載して」への対応。これまでaarch64(スマホ)は`inventory()`による命令セット検出のみで、実際の演算カーネル(`math.rs`)はすべてスカラー止まりだった。`dot_f32`/`axpy_f32`/`popcount_bytes`/`hamming_distance`にNEON実装(`vfmaq_f32`/`vcntq_u8`+`vaddlvq_u8`)を追加し、実機(arrows We2 PLUS M06)でdot_f32が2.83倍高速化することを確認した。あわせて`bit_impl_summary()`がARMでは常に「popcount: scalar」と誤表示していた(実際はNEON経由で2倍以上速いのに)不整合を発見・修正。x86_64(VPS/PC)側のAVX-512/AVX2+FMA3は既に実装済みで、今回VPS(Xeon Icelake)・ローカルPC(AMD Zen)・スマホ(Snapdragon)の3環境で実機ベンチマークを取り、いずれも正しくディスパッチされていることを確認した。

**English**: Per user instruction to actually leverage detected hardware (phone's core config, VPS's full AVX-512, local PC's AVX2/FMA3) instead of only reporting it. aarch64 previously had detection only; added real NEON kernels for dot_f32/axpy_f32/popcount_bytes/hamming_distance, verified 2.83x speedup live on a real Fujitsu arrows We2 PLUS M06. Also found and fixed a mismatch where bit_impl_summary() always printed "popcount: scalar" on ARM even while NEON was already accelerating it. x86_64 (VPS/PC) AVX-512/AVX2+FMA3 paths were already implemented; this session benchmarked all three real environments (VPS Xeon Icelake, local AMD PC, phone Snapdragon) and confirmed correct dispatch on each.

**简体中文**: 根据用户指示"充分利用手机的核心配置、VPS的完整AVX512指令集、本地PC的AVX2和FMA3等功能"而非仅仅报告它们。此前aarch64(手机)仅有指令集检测功能,`math.rs`中的实际计算内核全部停留在标量实现。为`dot_f32`/`axpy_f32`/`popcount_bytes`/`hamming_distance`添加了NEON实现,在真机(arrows We2 PLUS M06)上验证dot_f32提速2.83倍。同时发现并修复了`bit_impl_summary()`在ARM上始终错误显示"popcount: scalar"的问题(实际已通过NEON提速2倍以上)。

**繁體中文**: 根據使用者指示「充分利用手機的核心配置、VPS的完整AVX512指令集、本機PC的AVX2和FMA3等功能」而非僅僅回報它們。此前aarch64(手機)僅有指令集偵測功能,`math.rs`中的實際運算核心全部停留在純量實作。為`dot_f32`/`axpy_f32`/`popcount_bytes`/`hamming_distance`新增了NEON實作,在實機(arrows We2 PLUS M06)上驗證dot_f32提速2.83倍。同時發現並修正了`bit_impl_summary()`在ARM上始終錯誤顯示「popcount: scalar」的問題。

**Deutsch**: Gemäß Benutzeranweisung, die erkannte Hardware (Kernkonfiguration des Telefons, vollständiger AVX-512-Satz des VPS, AVX2/FMA3 des lokalen PCs) tatsächlich zu nutzen statt sie nur zu melden. aarch64 (Telefon) hatte zuvor nur Erkennung; echte NEON-Kernel für dot_f32/axpy_f32/popcount_bytes/hamming_distance hinzugefügt, live auf einem echten Fujitsu arrows We2 PLUS M06 eine 2,83-fache Beschleunigung bestätigt. Zudem eine Inkonsistenz in bit_impl_summary() gefunden und behoben, die auf ARM immer "popcount: scalar" anzeigte, obwohl NEON bereits beschleunigte.

**正直な開示/Honest disclosure**: このHANDOFF追記はトップレベルのCLAUDE.md(日本語正本)にのみ5ヶ国語で追記した。`README/`配下の言語別フル翻訳ファイル(14ヶ国語、README-China.md等)への反映は今回のスコープ外(既存の全文翻訳を都度追従させる運用は別途必要)。

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

- **2026-10-28 AVX-512 経路をIce Lake機(ConoHa VPS、Xeon Icelake 4コア)で実機検証**: `gf_mul_parity_avx512`・`dot_f32_avx512`・`axpy_f32_avx512` がスカラー実装と一致(端数長含む。直接呼ぶテスト `avx512_matches_scalar_when_actually_available` / `avx512_dot_and_axpy_match_scalar_when_actually_available` を追加。opt-in の有無に関係なく搭載機で実行される)。`select_prefers_first_satisfiable` の前提(「AVX-512VNNIは無い」)が搭載機で崩れていたテストの不具合も修正。
  **実測(同機、4MiB×50回の簡易ベンチ)**: GF(2^8)乗算は AVX2 12.0GiB/s ≈ AVX-512 11.4GiB/s(メモリ帯域律速で差なし)。`dot_f32` は AVX2+FMA3 17.0〜17.7 GFLOP/s に対し AVX-512 は 4累算器に直しても 11.8〜12.5 GFLOP/s(遅い)。`axpy` は差なし。**結論: この機では AVX-512 を既定にしない(opt-in のまま)。** 速くならない理由は未調査(帯域律速・クロック低下の可能性。推測)。VNNI の演算実装は未着手。
- **2026-10-28 命令セット上限 `OPEN_CPU_MAX_ISA`(AVX2機・古い機のフォールバックを実機で検証)**: `avx512`(既定・絞らない)/`avx2`(AVX-512系を無いことにする)/`ssse3`(AVX系も無し)/`scalar`(SIMD不使用)。`detect()` に反映されるので全ディスパッチが従う。純関数 `cap_isa` を公開し単体テスト3件。**Ice Lake VPSで4水準とも全テスト通過**(`avx2`: GF=AVX2 10.8〜12.0GiB/s・dot 17.4GFLOP/s、`ssse3`: GF=PCLMUL・dotはスカラー並み、`scalar`: 全部スカラー)。AVX-512機を持たなくても逆に低い水準を再現できる。AVX2しか無い実機(手元のRyzen 3950X)でも全テスト通過。AVX-512 は既定オフ(AVX2で動く)のまま。

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

## 2026-11-20 低VRAM・VRAM無しでの安定高速化のノウハウ / Low-VRAM know-how
日本語: VRAMが無い/小さいPC・VPSでAIを安定して速く動かす技術(分割重み・mmap・FP8・混合精度・小行列の直列化・プロンプト上限・同時実行上限、効かなかった手法も)は [aruaru-llm/docs/LOW-VRAM-KNOWHOW.md](https://github.com/aon-co-jp/aruaru-llm/blob/main/docs/LOW-VRAM-KNOWHOW.md) に集約。AIやLLMを扱う関連リポジトリはこれを参照し、同じ失敗を繰り返さない。
English: Techniques for running AI stably and fast on machines with no or little VRAM (sharded weights, mmap, FP8, mixed precision, serial small GEMMs, prompt caps, concurrency caps, plus what did not work) are collected in the file above. Related repos that use AI/LLMs should follow it.

## Versionless API の方針 / Versionless API policy (2026-11-25)
**日本語**: このリポジトリのAPIは **Versionless**(URL・パッケージ名に `v1` 等を付けず、互換性を壊さず足すだけで進化させる)。欄は足すだけ・足す欄は省略可能・知らない欄は無視・名前変更は新名を足して旧を非推奨に(期限は既定で6か月以上、使用0を確認してから削除)・enumは未知の値を許容・安全上の緊急修正は例外。既存の `.v1` は呼び出し側を壊さないよう当面残し、版なしの別名を追加して段階的に畳む。詳細・出典は [`runo/API-POLICY.md`](https://github.com/aon-co-jp/runo/blob/main/API-POLICY.md) 末尾。適用はこれから順に行う。
**English**: This repository's APIs are **Versionless**: no `v1` in URLs or package names; evolve by adding only, without breaking compatibility. Only add fields, new fields are optional, ignore unknown fields, rename by adding the new name and deprecating the old (default deadline 6 months or more, remove only after confirming zero use), tolerate unknown enum values, emergency security fixes are the exception. Existing `.v1` names stay for now so callers do not break; a versionless alias is added and `.v1` is retired in stages. Details and sources: the end of [`runo/API-POLICY.md`](https://github.com/aon-co-jp/runo/blob/main/API-POLICY.md). Rollout is step by step.
