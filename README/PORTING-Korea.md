> 일본어 원문 / 日本語原文: [PORTING.md](../PORTING.md)

# PORTING.md — 다른 저장소에서 `open-cpu` 로 갈아타는 절차

`open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` 등이 개별적으로 갖고
있는 CPU 기능 검출 코드·GF(2^8) 연산 코드를 `open-cpu` 로 집약하기 위한 이행
절차.

## 0. 전제

`open-cpu` 는 **라이브러리 크레이트**이며, 상주 서비스가 아니다.
프로세스 간 통신이나 별도 프로세스의 기동은 불필요하며, `Cargo.toml` 에
의존을 추가하고 함수를 호출하는 것만으로 완결된다.

## 1. 의존의 추가

로컬 작업 드라이브 `F:\runo` 아래에서는 path 의존이 가장 간단하다:

```toml
[dependencies]
open-cpu = { path = "../open-cpu" }
```

워크스페이스 내의 멤버 크레이트에서 사용하는 경우에는, 워크스페이스 root 의
`Cargo.toml` 에

```toml
[workspace.dependencies]
open-cpu = { path = "../open-cpu" }
```

라고 쓰고, 멤버 쪽에서 `open-cpu = { workspace = true }` 로 하는 것이
바람직하다.

향후 git 의존으로 전환하는 경우:

```toml
open-cpu = { git = "https://github.com/aon-co-jp/open-cpu", branch = "main" }
```

크레이트 이름은 하이픈이 붙은 `open-cpu`, Rust 에서 참조할 때의 경로는
언더스코어가 붙은 `open_cpu`.

## 2. CPU 기능 검출 코드의 치환

이행 전(각 저장소에 흔히 있는 패턴):

```rust
static HAS_AVX2: OnceLock<bool> = OnceLock::new();
fn has_avx2() -> bool {
    *HAS_AVX2.get_or_init(|| is_x86_feature_detected!("avx2"))
}
```

이행 후:

```rust
if open_cpu::detect().avx2 { /* ... */ }
```

`detect()` 는 내부에서 `OnceLock` 캐시가 되어 있으므로, 호출하는 쪽에서 다시
캐시할 필요는 없다. `&'static CpuCapabilities` 를 반환하기 때문에 할당도
발생하지 않는다.

이용 가능한 필드: `avx2` `avx512f` `avx512bw` `avx512vl` `pclmulqdq`
`bmi1` `bmi2` `fma` `aes` `popcnt` `sha` `sse2` `ssse3` `avx_vnni` `avx512vnni`.

## 3. GF(2^8) / 패리티 연산의 치환(주로 `open-raid-z`)

`open-cpu` 의 기약 다항식은 `0x11d`, 생성원은 `g = 2` 로, Linux md/RAID6 및
ZFS RAID-Z 와 동일하다. **이행 전에, 자기 저장소의 다항식·생성원이 이것과
일치하는지 반드시 확인할 것.** 다를 경우 수치가 맞지 않게 된다.

| 이행 전에 흔한 형태 | 이행 후 |
|---|---|
| `for i in .. { p[i] ^= d[i] }` | `open_cpu::gf_xor(&mut p, &d)` |
| `for i in .. { q[i] ^= gf_mul(d[i], c) }` | `open_cpu::gf_mul_parity(&mut q, &d, c)` |
| `for i in .. { acc[i] = mul2(acc[i]) ^ d[i] }` | `open_cpu::gf_mul2_xor(&mut acc, &d)` |
| `for i in .. { acc[i] = mul4(acc[i]) ^ d[i] }` | `open_cpu::gf_mul4_xor(&mut acc, &d)` |
| 임의 횟수의 `×2^n` 호너 법 | `open_cpu::gf_mul_pow2_xor(&mut acc, &d, n)` |
| 독자적인 `gf_mul(a: u8, b: u8) -> u8` | `open_cpu::gf_mul(a, b)`(`const fn`) |
| 독자적인 `mul2_byte(b) -> u8` | `open_cpu::gf_mul2_byte(b)`(`const fn`) |
| 독자적인 `g^i` 계수 계산 | `open_cpu::raid6_coeff(i)` |
| P/Q 일괄 계산 | `open_cpu::raid6_parity(&stripes, &mut p, &mut q)` |
| P/Q/R 일괄 계산(RAID-Z3 상당) | `open_cpu::raid6_parity3(&stripes, &mut p, &mut q, &mut r)` |

`gf_mul_parity` / `gf_xor` 는 `dst.len() != src.len()` 에서 panic 한다.
호출하는 쪽에서 스트라이프 길이를 맞춰 둘 것.

특정 구현을 명시적으로 호출하고 싶은 경우(벤치나 검증 목적):

- `open_cpu::gf_mul_parity_scalar(...)` / `gf_xor_scalar(...)` /
  `gf_mul_pow2_xor_scalar(...)` — safe
- `unsafe { open_cpu::gf_mul_parity_avx2(...) }` — 호출한 쪽이 AVX2 대응을 보증
- `unsafe { open_cpu::gf_mul_parity_pclmul(...) }` — 동일하게 SSSE3+PCLMULQDQ
- `unsafe { open_cpu::gf_mul_parity_avx512(...) }` — **실행 미검증**

## 4. 이행 후의 검증(필수)

1. `cargo build` — 의존 해결과 빌드가 통과할 것.
2. `cargo test` — **기존 테스트가 전부 통과할 것**. 특히, 치환 전후로 패리티의
   바이트열이 완전히 일치하는지 확인한다. 기존 테스트에 패리티 값의 비교가
   없다면, 이행 시에 추가할 것.
3. 로그에 `open_cpu::runtime_summary()` 를 1행 출력해 두면, 실제 기기에서
   어느 구현이 선택되었는지 나중에 확인할 수 있다.

## 4.5 실제 이행 사례(`open-raid-z`, 2026-08-22)

참고로, 최초 이행 실례의 요점을 든다:

- `detect_level()` 내의 `std::is_x86_feature_detected!` 를
  `open_cpu::detect()` 의 참조로 치환했다(`SimdLevel` 이라는 저장소 고유의
  열거형은 그대로 남기고, 그 **판정 재료만**을 open-cpu 로 옮겼다). 기존
  호출하는 쪽을 전혀 변경하지 않아도 된다.
- `gf_mul_xor_into()` / `xor_into()` / `mul_pow2_xor_into()` 의
  **AVX2 경로만**을 open-cpu 로 위임하고, AVX-512 경로(open-cpu 쪽도
  미검증)와 SSE2 경로(open-cpu 에 구현이 없음)는 저장소 쪽의 구현을 남겼다.
  **성능이 떨어지는 치환은 하지 않는다**는 판단.
- 사용되지 않게 된 SIMD 커널은 삭제하지 않고 `#[allow(dead_code)]` 를 붙여
  잔치하고, 향후 open-cpu 쪽과 상호 검증할 때의 참조로 삼았다.

이 「전부를 한 번에 치환하지 않고, 등가이면서 성능이 떨어지지 않는 부분부터
단계적으로 위임한다」는 진행 방식을 권장한다.

## 5. 주의점

- **AVX-512 경로는 실행 미검증**(개발 머신이 미탑재). 기본으로는 선택되지
  않으므로, 이행에 의해 동작이 바뀌는 일은 없다. AVX-512 머신에서 검증하는
  경우에만 `OPEN_CPU_ENABLE_AVX512=1` 을 설정한다.
- POPCNT/BMI/FMA/AES-NI/SHA-NI/VNNI 는 **검출만**. 이들을 사용한 체크섬·
  압축·행렬 연산의 구현은 `open-cpu` 에 아직 없으므로, `aruaru-db` /
  `aruaru-llm` 쪽의 해당 코드는 아직 이행할 수 없다(검출 부분만 먼저 이행하는
  것은 가능).
- x86 이외(ARM 등)에서는 모든 기능이 `false` 가 되어 스칼라 구현으로
  떨어진다. 크로스 빌드는 통과하지만, NEON 등의 최적화는 미대응.
