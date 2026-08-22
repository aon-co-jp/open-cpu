> 일본어 원문 / 日本語原文: [README.md](../README.md)

# open-cpu

`aon-co-jp` 에코시스템 공통의 **CPU 명령어 세트 검출·런타임 디스패치
라이브러리**(Rust).

`open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` 가 각각 독자적으로
CPU 기능 검출 코드를 작성하여 중복되는 것을 피하기 위해 신설했다.

**상주 서비스(데몬)가 아니다.** 각 저장소가 `Cargo.toml` 의
`[dependencies]` 에 추가하고, 동일 프로세스 내로 링크하여 사용하는 일반적인
라이브러리 크레이트이다.

## 할 수 있는 것

1. **CPU 기능의 런타임 검출** — `open_cpu::detect()` 가
   `&'static CpuCapabilities` 를 반환한다. `std::is_x86_feature_detected!` 를
   사용하며, 최초 검출 결과를 `OnceLock` 에 캐시하므로 몇 번을 호출해도
   비용은 거의 0에 가깝다.
2. **RAID6 GF(2^8) 연산의 런타임 디스패치** — 검출 결과에 따라
   스칼라 / PCLMULQDQ / AVX2 / AVX-512 구현을 실행 시점에 선택한다.

## 사용법

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

### 공개 API 목록

| API | 내용 | 디스패치 |
|---|---|---|
| `detect() -> &'static CpuCapabilities` | CPU 기능 검출(`OnceLock` 캐시) | — |
| `runtime_summary() -> String` | 검출 결과 + 선택된 구현의 1행 요약 | — |
| `selected_impl() -> GfImpl` | GF 연산에서 선택된 구현 | — |
| `gf_xor(dst, src)` | `dst ^= src`(P 패리티) | AVX2 / 스칼라 |
| `gf_mul_parity(dst, src, factor)` | `dst ^= src * factor`(Q 패리티) | AVX-512(opt-in) / AVX2 / PCLMULQDQ / 스칼라 |
| `gf_mul_pow2_xor(acc, src, times)` | `acc = acc * 2^times ^ src` | AVX2 / 스칼라 |
| `gf_mul2_xor` / `gf_mul4_xor` | 위의 `times=1` / `times=2` | 위와 동일 |
| `raid6_parity(stripes, p, q)` | P/Q 를 계수 테이블 방식으로 일괄 계산 | 위에 준함 |
| `raid6_parity3(stripes, p, q, r)` | P/Q/R 을 호너 법으로 일괄 계산 | 위에 준함 |
| `gf_mul(a, b) -> u8` | 1바이트 GF 곱셈(`const fn`) | — |
| `gf_mul2_byte(b) -> u8` | 1바이트의 GF 상 2배(`const fn`) | — |
| `raid6_coeff(i) -> u8` | RAID6 의 계수 `g^i`(`g = 2`) | — |

각 구현을 명시적으로 호출하는 `*_scalar` / `*_avx2` / `*_pclmul` / `*_avx512`
버전도 공개하고 있다(벤치·상호 검증용, SIMD 버전은 `unsafe`).

## 검출 대상 명령어 세트

| 명령어 세트 | 검출 | 이 크레이트에서의 이용 |
|---|---|---|
| SSE2 | ✅ | PCLMULQDQ 경로의 보조로 사용 |
| SSSE3 | ✅ | `pshufb`(PCLMULQDQ 경로의 환원) |
| PCLMULQDQ | ✅ | GF(2^8) 곱셈(캐리리스 곱셈 구현) |
| AVX2 | ✅ | GF(2^8) 곱셈(`vpshufb` split-table, 기본으로 선택) |
| AVX-512F / BW / VL | ✅ | GF(2^8) 곱셈 경로 있음(**실행 미검증**, 아래 참조) |
| POPCNT | ✅ | 검출만(이용하는 구현 없음) |
| BMI1 / BMI2 | ✅ | 검출만(이용하는 구현 없음) |
| FMA3 | ✅ | 검출만(이용하는 구현 없음) |
| AES-NI | ✅ | 검출만(이용하는 구현 없음) |
| SHA-NI | ✅ | 검출만(이용하는 구현 없음) |
| AVX-VNNI | ✅ | 검출만(향후 AI 추론용, 이용하는 구현 없음) |
| AVX-512 VNNI | ✅ | 검출만(향후 AI 추론용, 이용하는 구현 없음) |

x86/x86_64 이외의 아키텍처에서는 모든 필드가 `false` 가 되며, 스칼라 구현으로
폴백한다(빌드는 통과한다).

## GF(2^8) 구현의 상세

기약 다항식은 `0x11d`(x^8+x^4+x^3+x^2+1), 생성원은 `g = 2`.
Linux md/RAID6 및 ZFS RAID-Z 와 동일하다.

- **스칼라**: nibble split 테이블(16 엔트리 × 2)에 의한 참조 구현.
- **PCLMULQDQ**: 각 바이트를 16bit 간격으로 전개하면, 8bit 계수와의 캐리리스
  곱(최대 15bit)이 인접 슬롯으로 자리올림되지 않는다. 이 성질을 이용해 1개의
  명령으로 4바이트분을 한꺼번에 곱하고, `pshufb` 2회로 GF(2^8) 로 환원한다.
  16 byte/iter.
- **AVX2**: `vpshufb` 에 의한 split-table 구현. 32 byte/iter.
- **AVX-512F/BW**: 동일한 split-table 을 64 byte/iter 로 처리.

## 실측 벤치마크

`cargo run --release --example bench`(4 MiB × 50 회 = 200 MiB, factor=0x8d)

개발 머신: **AMD Ryzen 9 3950X** / Windows 11 / rustc 1.96.0

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

**실측값의 편차에 대하여(정직한 기록)**: 4회 연속 실행한 결과, GF 곱셈의
AVX2 배율은 **11.6〜18.1 배**, 호너 법은 **2.70〜3.52 배**, XOR 은
**1.12〜1.25 배** 의 범위에서 변동했다(스칼라 쪽은 207 ms 전후로 안정).
SIMD 쪽은 1만〜2만 MiB/s 에 도달하여 **메모리 대역 율속**이 되고 있기 때문에,
캐시 상태나 다른 프로세스의 영향을 받기 쉽다. 위 표는 1회 실행 결과를 그대로
실은 것이며, 배율은 범위로 파악하는 것이 정확하다.

- **GF(2^8) 임의 계수 곱셈은 AVX2 에서 스칼라 대비 11.6〜18.1 배(실측)**.
  RAID6 의 Q 패리티·복구 경로에서 효과가 있다.
- **호너 법은 2.70〜3.52 배(실측)**. 스칼라 버전이 이미 u64 비트 트릭으로
  최적화되어 있기 때문에, GF 곱셈만큼의 차이는 나지 않는다.
- **단순 XOR 은 1.12〜1.25 배**. 스칼라(u64) 시점에서 이미 메모리 대역에
  붙어 있기 때문에, SIMD 화의 여지가 작다(예상대로).
- PCLMULQDQ 는 1.90 배로, **AVX2 를 사용할 수 없는 오래된 CPU 용 폴백**
  으로서만 의미가 있다.

## 검증 상황(정직한 공개)

- ✅ **스칼라 / PCLMULQDQ / AVX2**: 위 개발 머신에서 실행 검증 완료.
  `cargo test` 에서, 소박한 비트 시프트 구현을 기준으로 스칼라 구현의 정확성을,
  다시 스칼라 구현을 기준으로 PCLMULQDQ 구현(**전체 256가지 계수 × 9가지
  길이**)과 AVX2 구현(8가지 계수 × 11가지 길이, 끝수 처리 포함)의 출력 일치를
  확인하고 있다. 전체 15개 테스트 + doctest 2건이 통과.
- ⚠️ **AVX-512 경로는 실행 미검증**. 개발 머신(Ryzen 9 3950X)이 AVX-512
  미탑재이기 때문에, **컴파일이 통과하는 것만** 확인하고 있다. 안전을 위해
  기본 디스패치에서는 선택되지 않으며, 환경 변수
  `OPEN_CPU_ENABLE_AVX512=1` 을 설정한 경우에만 opt-in 으로 유효해진다.
  AVX-512 탑재 머신에서 검증이 끝날 때까지 이 취급을 유지한다.
- ⚠️ POPCNT/BMI1/BMI2/FMA/AES-NI/SHA-NI/VNNI 는 **검출 필드가 있을 뿐**이며,
  이들을 사용한 연산 구현은 아직 없다.

## 테스트·벤치 실행

```
cargo build --release
cargo test --release
cargo run --release --example bench
```

## 도입 실적(2026-08-22 시점)

| 저장소 | 사용법 |
|---|---|
| [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z) | `zfs_accel_hlsl/src/simd.rs` 의 CPU 기능 검출·GF(2^8) 곱셈·XOR·호너 법(AVX2 경로)을 본 크레이트에 위임. 이행 후에도 기존 39개 테스트가 모두 통과하며, 수치는 완전히 일치. |
| [`open-english`](https://github.com/aon-co-jp/open-english) | 서버 기동 로그의 1행 요약과, `GET /v1/cpu-runtime`(실행 기반의 CPU 명령어 세트를 JSON 으로 반환). |

미도입(향후 대상): `aruaru-db`(체크섬·압축),
`aruaru-llm`(행렬 연산), `open-cuda`(GPU 부재 시의 CPU 폴백).

## 관련

- 이행 절차: [PORTING.md](PORTING-Korea.md)
- 개발 방침·HANDOFF: [CLAUDE.md](CLAUDE-Korea.md)
- GitHub organization: https://github.com/aon-co-jp
