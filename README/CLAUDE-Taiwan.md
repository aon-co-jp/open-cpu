> 日文原文 / 日本語原文: [CLAUDE.md](../CLAUDE.md)

# 開發方針與開發環境規則(open-cpu)

本儲存庫是 `aon-co-jp` 生態系的一員。**所有儲存庫共通的設計思想與
運用規則(徹底驗證、禁止誇大回報、避免重造輪子、無須確認的自動接續等)
以 [`open-raid-z/CLAUDE.md`](https://github.com/aon-co-jp/open-raid-z/blob/main/CLAUDE.md)
為正本。** 此處不再複製,僅記載本儲存庫特有的事項。

工作磁碟為 `F:\runo\open-cpu`(新配置)。

## 本儲存庫的角色

由於 `open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` **各自**打算實作
CPU 指令集(AVX2/AVX-512/PCLMULQDQ/BMI1/BMI2/FMA3/AES-NI/POPCNT/SHA-NI)的
執行期偵測與分派,因此新設本儲存庫作為其共通基礎。

- **它是函式庫 crate**(使用者指示,2026-08-22)。不做成常駐型服務
  (daemon)。各儲存庫在 `[dependencies]` 中加入,連結進同一個行程內使用。
  不要導入行程間通訊。
- 範圍僅限於「CPU 功能偵測」與「依該偵測結果進行的運算分派」。
  不要把各儲存庫的領域邏輯(RAID 的分條策略、DB 的分頁管理、
  LLM 的模型載入等)移植到這裡。
- 不增加相依 crate(目前 `[dependencies]` 為空,僅使用 `std`)。
  由於它是位於各儲存庫最底層的基礎元件,一旦帶進相依就會波及全體。

## 實作與驗證規則(本儲存庫特有)

- **一旦新增 SIMD 實作,務必同時撰寫與純量實作的輸出一致性測試。**
  必須把畸零部分(長度並非向量寬度倍數的情形)納入測試案例。
- **無法在實機上執行的程式碼路徑要明確標示「未驗證」,並且不讓預設的
  分派選用它。** 目前 AVX-512 路徑屬於此類(開發機的
  AMD Ryzen 9 3950X 並未搭載 AVX-512)。只有透過 `OPEN_CPU_ENABLE_AVX512=1`
  自行啟用時才會生效。在搭載 AVX-512 的機器上完成驗證的時間點,應解除此限制,
  並更新 README.md / PORTING.md / 本檔案中「未驗證」的標示。
- 撰寫效能量測的數值時,**只把實際量測到的數值**寫成「實測」。
  不要把推估值、理論值寫得像實測值一樣。
- 不使用 `cargo bench`,以 `examples/bench.rs` 中基於 `std::time::Instant` 的
  簡易量測即已足夠(以免增加相依)。

## 多語言文件

在 `README/` 資料夾中放置了 15 種語言版本的 README / CLAUDE / PORTING
(生態系共通的做法,與 `open-raid-z` / `open-cuda` 採用相同的命名規則)。
**日文版(儲存庫根目錄下的 `README.md` / `CLAUDE.md` / `PORTING.md`)為
正本**,更新內容時也要讓 15 種語言版本跟進
(沒有自動同步機制,須以人工反映)。

語言: US English / UK English / Germany / Italy / France / Spain / Russia /
Ukraine / Hebrew / Persian(Iran) / Arabic / China / Taiwan / Korea / Japan。

## HANDOFF

- **2026-08-22 新建 + 整合至 2 個儲存庫**:
  從空儲存庫的狀態建立了初始實作。
  - `src/caps.rs`: `CpuCapabilities` 結構與 `detect()`(`OnceLock` 快取)。
    偵測對象為 avx2 / avx512f / avx512bw / avx512vl / pclmulqdq / bmi1 / bmi2 /
    fma / aes / popcnt / sha / sse2 / ssse3 / avx-vnni / avx512vnni。
  - `src/gf.rs`: RAID6 的 GF(2^8) 運算(多項式 0x11d,生成元 g=2)。
    公開 `gf_xor` / `gf_mul_parity` / `raid6_parity` / `raid6_coeff`,
    並在執行期分派純量 / PCLMULQDQ / AVX2 / AVX-512 這 4 種實作。
  - 實測(Ryzen 9 3950X,4MiB×50 次): scalar 1003 MiB/s、pclmulqdq 1916 MiB/s
    (1.91x)、**avx2 22531 MiB/s(22.46x)**。AVX-512 因未搭載而無法實測。
    → **後來重新量測後判明,此 avx2 的倍率會在 11.6〜18.1 倍的範圍內變動**
    (因為 SIMD 側受記憶體頻寬限制。最新的數值與範圍以
    README.md 的「實測效能量測」一節為準)。
  - `cargo test --release` 全部 11 項測試 + 1 件 doctest 通過。PCLMULQDQ 實作
    已確認在全部 256 種係數下皆與純量一致。
  - 整合實績: 已以 path 相依方式納入 `open-raid-z`(GF 運算的替換)與
    `open-english/server`(`/v1/cpu-runtime` 端點與啟動記錄)。

- **2026-08-22(續)實用性提升循環 + 自我介紹功能**:
  在初始實作之後,實施了為提高協同性與實用性的 開發→TEST→修正 循環。
  - **循環 1: 新增霍納法 API**(`gf_mul_pow2_xor` /
    `gf_mul2_xor` / `gf_mul4_xor`,AVX2 + 純量)。藉此
    `open-raid-z` 的 `mul2_xor_into` / `mul4_xor_into` 的 AVX2 路徑也能
    委由 open-cpu 處理,進一步削減了該側的 x86 程式碼。
    也為 `gf_xor` 新增了 AVX2 路徑(以免委由處理反而造成效能退步)。
  - **循環 2: 改善易用性**。為 `CpuCapabilities` 新增 `Display` 實作與
    `has_all()`。新增可一次計算相當於 RAID-Z3 的 P/Q/R 的
    `raid6_parity3()`。在 `examples/bench.rs` 中新增了 XOR 與霍納法的
    量測。
  - **驗證**: `cargo test --release` **全部 15 項測試 + 2 件 doctest 通過**。
    效能量測連續執行 4 次,並把變動(因 SIMD 側受記憶體頻寬限制而
    在 11.6〜18.1 倍之間變動)誠實地以範圍形式記錄於 README。
  - **在整合至 `open-english` 時發現的實際 bug(重要教訓)**: 把「是誰
    做的」這項自我介紹回應功能以關鍵字部分比對
    (`"誰が作"`)實作後,**在真實瀏覽器的測試中發現
    「誰が【このシステムを】作ったのですか?」無法被偵測到**
    (因為疑問詞與動詞之間夾入了語句)。已改為把疑問詞清單與動詞
    清單分開並以 AND 條件判定,並進行了 10 例肯定、6 例否定的
    確認以及真實瀏覽器上的 E2E 確認。這是一項記錄: **這類 bug 不是靠
    「單元測試通過了」,而是要做到「實際在聊天中輸入看看」
    才找得出來**。

- **接下來該做的事**:
  1. 逐步整合到 `aruaru-db` 的檢查碼與壓縮相關部分、`aruaru-llm` 的矩陣運算、
     `open-cuda` 的 CPU 回退(目前只有
     `open-raid-z` 與 `open-english` 這 2 件)。
  2. 若能取得、確保搭載 AVX-512 的機器,就對 AVX-512 路徑進行實測驗證,
     並解除自行啟用的限制。
  3. 對於只有偵測而無實作的指令集(POPCNT/BMI/FMA/AES-NI/SHA-NI),
     從相依側實際需要的項目開始新增運算實作。特別是
     `aruaru-db` 的檢查碼可考慮以 PCLMULQDQ 實作 CRC32C,
     `aruaru-llm` 則可考慮以 AVX-VNNI/AVX-512 VNNI 實作 int8 內積。
  4. 由於是以 path 相依(`path = "../open-cpu"`)組成,在 VPS 等
     沒有 `F:\runo` 配置的環境中建置時,需要切換為 git 相依。
     等到實際要在 VPS 上建置時再行處理。
  5. 由於沒有針對僅支援 SSE2 的 CPU 的實作,`open-raid-z` 僅在 SSE2 路徑上
     保留了自有實作。若新增 SSE2 版本,那部分也能委由本 crate 處理。
  6. `gf_xor` 即使 SIMD 化,相對於純量也只有 1.12〜1.25 倍
     (受記憶體頻寬限制)。以非暫存儲存(`_mm256_stream_si256`)
     避免快取汙染的最佳化有可能奏效,針對大型緩衝區值得一試(未驗證)。
