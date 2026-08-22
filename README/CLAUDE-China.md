> 日文原文 / 日本語原文: [CLAUDE.md](../CLAUDE.md)

# 开发方针与开发环境规则(open-cpu)

本仓库是 `aon-co-jp` 生态系统的一员。**全部仓库通用的设计思想与
运用规则(彻底验证、禁止夸大报告、避免重复造轮子、无需确认的自动继续等)
以 [`open-raid-z/CLAUDE.md`](https://github.com/aon-co-jp/open-raid-z/blob/main/CLAUDE.md)
为正本。** 此处不再复制,仅记载本仓库特有的事项。

工作驱动器为 `F:\runo\open-cpu`(新布局)。

## 本仓库的角色

由于 `open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` **各自**打算实现
CPU 指令集(AVX2/AVX-512/PCLMULQDQ/BMI1/BMI2/FMA3/AES-NI/POPCNT/SHA-NI)的
运行时检测与分派,故新建本仓库作为其共通基础。

- **它是库 crate**(用户指示,2026-08-22)。不做成常驻型服务
  (守护进程)。各仓库在 `[dependencies]` 中添加,链接进同一进程内使用。
  不要引入进程间通信。
- 作用范围仅限于「CPU 特性检测」与「基于该检测结果的运算分派」。
  不要把各仓库的领域逻辑(RAID 的条带化策略、DB 的页管理、
  LLM 的模型加载等)移植到这里。
- 不增加依赖 crate(目前 `[dependencies]` 为空,仅使用 `std`)。
  由于它是位于各仓库最底层的基础组件,一旦引入依赖就会波及全体。

## 实现与验证规则(本仓库特有)

- **一旦添加 SIMD 实现,必须同时编写与标量实现的输出一致性测试。**
  必须把尾数(长度不是向量宽度整数倍的情况)包含进测试用例。
- **无法在实机上执行的代码路径要明确标注「未验证」,并且不让默认分派
  选择它。** 目前 AVX-512 路径属于此类(开发机的
  AMD Ryzen 9 3950X 不支持 AVX-512)。只有通过 `OPEN_CPU_ENABLE_AVX512=1`
  显式启用时才生效。在配备 AVX-512 的机器上完成验证之时,应解除此限制,
  并更新 README.md / PORTING.md / 本文件中的「未验证」表述。
- 书写基准测试数值时,**只把实际测得的数值**写成「实测」。
  不要把推定值、理论值写得像实测值一样。
- 不使用 `cargo bench`,用 `examples/bench.rs` 中基于 `std::time::Instant` 的
  简易计测即可(以免增加依赖)。

## 多语言文档

在 `README/` 文件夹中放置了 15 种语言版本的 README / CLAUDE / PORTING
(生态系统通用的做法,与 `open-raid-z` / `open-cuda` 采用相同的命名规则)。
**日语版(仓库根目录下的 `README.md` / `CLAUDE.md` / `PORTING.md`)为
正本**,更新内容时也要让 15 种语言版本随之跟进
(没有自动同步机制,需手动反映)。

语言: US English / UK English / Germany / Italy / France / Spain / Russia /
Ukraine / Hebrew / Persian(Iran) / Arabic / China / Taiwan / Korea / Japan。

## HANDOFF

- **2026-08-22 新建 + 集成到 2 个仓库**:
  从空仓库的状态创建了初始实现。
  - `src/caps.rs`: `CpuCapabilities` 结构体与 `detect()`(`OnceLock` 缓存)。
    检测对象为 avx2 / avx512f / avx512bw / avx512vl / pclmulqdq / bmi1 / bmi2 /
    fma / aes / popcnt / sha / sse2 / ssse3 / avx-vnni / avx512vnni。
  - `src/gf.rs`: RAID6 的 GF(2^8) 运算(多项式 0x11d,生成元 g=2)。
    公开 `gf_xor` / `gf_mul_parity` / `raid6_parity` / `raid6_coeff`,
    并在运行时分派标量 / PCLMULQDQ / AVX2 / AVX-512 这 4 种实现。
  - 实测(Ryzen 9 3950X,4MiB×50 次): scalar 1003 MiB/s、pclmulqdq 1916 MiB/s
    (1.91x)、**avx2 22531 MiB/s(22.46x)**。AVX-512 因不支持而无法实测。
    → **后来的重新测量表明,该 avx2 的倍率会在 11.6〜18.1 倍的范围内变动**
    (因为 SIMD 一侧受内存带宽限制。最新的数值与范围以
    README.md 的「实测基准测试」一节为准)。
  - `cargo test --release` 全部 11 项测试 + 1 项 doctest 通过。PCLMULQDQ 实现
    已确认在全部 256 种系数下与标量一致。
  - 集成实绩: 已通过 path 依赖嵌入到 `open-raid-z`(GF 运算的替换)与
    `open-english/server`(`/v1/cpu-runtime` 端点与启动日志)。

- **2026-08-22(续)实用性提升循环 + 自我介绍功能**:
  在初始实现之后,实施了为提高协同性与实用性的 开发→TEST→修正 循环。
  - **循环 1: 追加霍纳法 API**(`gf_mul_pow2_xor` /
    `gf_mul2_xor` / `gf_mul4_xor`,AVX2 + 标量)。由此
    `open-raid-z` 的 `mul2_xor_into` / `mul4_xor_into` 的 AVX2 路径也能
    委托给 open-cpu,从而进一步削减了那边的 x86 代码。
    也为 `gf_xor` 追加了 AVX2 路径(以免委托导致性能退化)。
  - **循环 2: 改善易用性**。为 `CpuCapabilities` 追加了 `Display` 实现与
    `has_all()`。追加了批量计算相当于 RAID-Z3 的 P/Q/R 的
    `raid6_parity3()`。在 `examples/bench.rs` 中追加了 XOR 与霍纳法的
    计测。
  - **验证**: `cargo test --release` **全部 15 项测试 + 2 项 doctest 通过**。
    基准测试连续执行 4 次,并把波动(因 SIMD 一侧受内存带宽限制而
    在 11.6〜18.1 倍之间变动)如实地以范围形式记录到了 README 中。
  - **在集成到 `open-english` 时发现的真实 bug(重要教训)**: 把「是谁
    做的」这一自我介绍应答功能实现为关键词部分匹配
    (`"誰が作"`)后,**在真实浏览器的测试中发现
    「誰が【このシステムを】作ったのですか?」无法被检测到**
    (因为疑问词与动词之间夹入了词句)。改为把疑问词列表与动词
    列表分开并以 AND 条件判定,并进行了 10 例肯定、6 例否定的
    确认以及真实浏览器上的 E2E 确认。这是一条记录: **这类 bug 不是靠
    「单元测试通过了」,而是要做到「实际往聊天里输入试试」
    才能发现**。

- **接下来应做的事**:
  1. 逐步集成到 `aruaru-db` 的校验和与压缩相关部分、`aruaru-llm` 的矩阵运算、
     `open-cuda` 的 CPU 回退中(目前仅有
     `open-raid-z` 与 `open-english` 这 2 例)。
  2. 若能获取、确保配备 AVX-512 的机器,则对 AVX-512 路径进行实测验证,
     并解除显式启用的限制。
  3. 对于仅有检测而无实现的指令集(POPCNT/BMI/FMA/AES-NI/SHA-NI),
     从依赖方实际需要的那些开始追加运算实现。特别是
     `aruaru-db` 的校验和可考虑用 PCLMULQDQ 实现 CRC32C,
     `aruaru-llm` 可考虑用 AVX-VNNI/AVX-512 VNNI 实现 int8 内积。
  4. 由于是以 path 依赖(`path = "../open-cpu"`)构建的,在 VPS 等
     没有 `F:\runo` 布局的环境中构建时,需要切换为 git 依赖。
     等到实际要在 VPS 上构建时再处理。
  5. 由于没有面向仅支持 SSE2 的 CPU 的实现,`open-raid-z` 仅在 SSE2 路径上
     保留了自有实现。若追加 SSE2 版本,那部分也可以委托过来。
  6. `gf_xor` 即使 SIMD 化,相对标量也只有 1.12〜1.25 倍
     (受内存带宽限制)。用非临时存储(`_mm256_stream_si256`)
     避免缓存污染的优化有可能奏效,面向大缓冲区值得一试(未验证)。
