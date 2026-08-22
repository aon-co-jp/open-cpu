> 일본어 원문 / 日本語原文: [CLAUDE.md](../CLAUDE.md)

# 개발 방침 & 개발 환경 규칙(open-cpu)

이 저장소는 `aon-co-jp` 에코시스템의 일원이다. **전 저장소 공통의 설계
사상·운용 규칙(검증 철저, 과장 보고 금지, 바퀴의 재발명 회피, 확인 불요의
자동 계속 등)은
[`open-raid-z/CLAUDE.md`](https://github.com/aon-co-jp/open-raid-z/blob/main/CLAUDE.md)
를 정본으로 한다.** 여기에는 복제하지 않고, 이 저장소 고유의 사항만 기재한다.

작업 드라이브는 `F:\runo\open-cpu`(신규 레이아웃).

## 이 저장소의 역할

`open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` 가 **각각 독자적으로**
CPU 명령어 세트(AVX2/AVX-512/PCLMULQDQ/BMI1/BMI2/FMA3/AES-NI/POPCNT/SHA-NI)의
런타임 검출·디스패치를 구현하려 했기 때문에, 그 공통 기반으로서 신설했다.

- **라이브러리 크레이트이다**(사용자 지시, 2026-08-22). 상주형 서비스
  (데몬)로 만들지 않는다. 각 저장소가 `[dependencies]` 에 추가하고, 동일
  프로세스 내로 링크하여 사용한다. 프로세스 간 통신은 도입하지 말 것.
- 스코프는 「CPU 기능 검출」과 「그 검출 결과에 근거한 연산 디스패치」로
  한정한다. 각 저장소의 도메인 로직(RAID 의 스트라이핑 전략, DB 의 페이지
  관리, LLM 의 모델 로딩 등)을 여기로 이식하지 말 것.
- 의존 크레이트를 늘리지 않는다(현재 `[dependencies]` 는 비어 있으며, `std`
  만). 각 저장소의 최하층에 들어가는 기반이므로, 의존을 들여오면 전체로
  파급된다.

## 구현·검증 규칙(이 저장소 고유)

- **SIMD 구현을 추가했다면, 반드시 스칼라 구현과의 출력 일치 테스트를 동시에
  작성한다.** 끝수(벡터 폭의 배수가 아닌 길이)를 반드시 테스트 케이스에
  포함할 것.
- **실제 기기에서 실행할 수 없는 코드 경로는 「미검증」이라고 명기하고, 기본
  디스패치에서 선택되지 않게 한다.** 현재 AVX-512 경로가 이에 해당한다(개발
  머신인 AMD Ryzen 9 3950X 가 AVX-512 미탑재). `OPEN_CPU_ENABLE_AVX512=1` 에
  의한 opt-in 으로만 유효해진다. AVX-512 탑재 머신에서 검증이 끝난 시점에 이
  제한을 해제하고, README.md / PORTING.md / 본 파일의 「미검증」 표기를
  갱신할 것.
- 벤치마크의 수치를 적을 때는, **실측한 것만**을 「실측」이라고 적는다.
  추정값·이론값을 실측인 것처럼 적지 않는다.
- `cargo bench` 는 사용하지 않고, `examples/bench.rs` 의
  `std::time::Instant` 에 의한 간이 계측으로 충분하다(의존을 늘리지 않기
  위해).

## 다국어 문서

`README/` 폴더에 15개 언어판의 README / CLAUDE / PORTING 을 배치하고 있다
(에코시스템 공통의 운용, `open-raid-z` / `open-cuda` 와 동일한 명명 규칙).
**일본어판(저장소 바로 아래의 `README.md` / `CLAUDE.md` / `PORTING.md`)이
정본**이며, 내용을 갱신했을 때는 15개 언어판도 따라가게 할 것(자동 동기화
구조는 없으므로 수동으로 반영한다).

언어: US English / UK English / Germany / Italy / France / Spain / Russia /
Ukraine / Hebrew / Persian(Iran) / Arabic / China / Taiwan / Korea / Japan.

## HANDOFF

- **2026-08-22 신규 작성 + 2개 저장소로의 통합**:
  빈 저장소 상태에서 초기 구현을 작성했다.
  - `src/caps.rs`: `CpuCapabilities` 구조체와 `detect()`(`OnceLock` 캐시).
    검출 대상은 avx2 / avx512f / avx512bw / avx512vl / pclmulqdq / bmi1 /
    bmi2 / fma / aes / popcnt / sha / sse2 / ssse3 / avx-vnni / avx512vnni.
  - `src/gf.rs`: RAID6 의 GF(2^8) 연산(다항식 0x11d, 생성원 g=2).
    `gf_xor` / `gf_mul_parity` / `raid6_parity` / `raid6_coeff` 를 공개하고,
    스칼라 / PCLMULQDQ / AVX2 / AVX-512 의 4개 구현을 실행 시점 디스패치.
  - 실측(Ryzen 9 3950X, 4MiB×50회): scalar 1003 MiB/s, pclmulqdq 1916 MiB/s
    (1.91x), **avx2 22531 MiB/s(22.46x)**. AVX-512 는 미탑재이므로 실측 불가.
    → **이 avx2 의 배율은 후일의 재측정에서 11.6〜18.1 배의 범위로 변동한다는
    것이 판명되었다**(SIMD 쪽이 메모리 대역 율속이기 때문. 최신 수치와 범위는
    README.md 의 「실측 벤치마크」 절이 정본).
  - `cargo test --release` 전체 11개 테스트 + doctest 1건 통과. PCLMULQDQ
    구현은 전체 256개 계수에서 스칼라와 일치함을 확인 완료.
  - 통합 실적: `open-raid-z`(GF 연산의 치환)와 `open-english/server`
    (`/v1/cpu-runtime` 엔드포인트와 기동 로그)에 path 의존으로 편입 완료.

- **2026-08-22(계속) 실용성 향상 사이클 + 자기소개 기능**:
  초기 구현 이후, 연계성·실용성을 높이기 위한 개발→TEST→수정 사이클을
  실시했다.
  - **사이클1: 호너 법 API 의 추가**(`gf_mul_pow2_xor` / `gf_mul2_xor` /
    `gf_mul4_xor`, AVX2 + 스칼라). 이로써 `open-raid-z` 의
    `mul2_xor_into` / `mul4_xor_into` 의 AVX2 경로도 open-cpu 로 위임할 수
    있게 되어, 그쪽의 x86 코드를 한층 더 삭감할 수 있었다. `gf_xor` 에도
    AVX2 경로를 추가(위임이 성능 퇴행이 되지 않도록 하기 위해).
  - **사이클2: 사용 편의성의 개선**. `CpuCapabilities` 에 `Display` 구현과
    `has_all()` 을 추가. RAID-Z3 상당의 P/Q/R 을 일괄 계산하는
    `raid6_parity3()` 을 추가. `examples/bench.rs` 에 XOR·호너 법의 계측을
    추가.
  - **검증**: `cargo test --release` **전체 15개 테스트 + doctest 2건 통과**.
    벤치는 4회 연속 실행하여, 편차(SIMD 쪽이 메모리 대역 율속이기 때문에
    11.6〜18.1 배로 변동)를 README 에 정직하게 범위로 기록했다.
  - **`open-english` 로의 통합에서 발견한 실제 버그(중요한 교훈)**: 「누가
    만들었는가」에 대한 자기소개 응답 기능을 키워드 부분 일치(`"誰が作"`)로
    구현했더니, **실제 브라우저에서의 테스트에서
    「誰が【このシステムを】作ったのですか?」를 검출할 수 없다**는 것이
    판명되었다(의문사와 동사 사이에 어구가 들어가기 때문). 의문사 목록과
    동사 목록을 나누어 AND 조건으로 판정하는 형태로 수정하고, 긍정 10개 예·
    부정 6개 예의 확인과 실제 브라우저에서의 E2E 확인을 실시했다. **「유닛
    테스트가 통과했다」가 아니라 「실제로 채팅에 입력해 본다」는 데까지 하지
    않으면 발견되지 않는 종류의 버그**였다, 라는 기록.

- **다음에 해야 할 것**:
  1. `aruaru-db` 의 체크섬·압축 관련, `aruaru-llm` 의 행렬 연산,
     `open-cuda` 의 CPU 폴백에도 순차적으로 통합한다(현재는 `open-raid-z` 와
     `open-english` 의 2건뿐).
  2. AVX-512 탑재 머신을 입수·확보할 수 있게 되면 AVX-512 경로를 실측
     검증하고, opt-in 제한을 해제한다.
  3. 검출만 있고 구현이 없는 명령어 세트(POPCNT/BMI/FMA/AES-NI/SHA-NI)에
     대해서는, 의존하는 쪽에서 실제로 필요해진 것부터 연산 구현을 추가한다.
     특히 `aruaru-db` 의 체크섬에는 PCLMULQDQ 에 의한 CRC32C,
     `aruaru-llm` 에는 AVX-VNNI/AVX-512 VNNI 에 의한 int8 내적이 후보.
  4. path 의존(`path = "../open-cpu"`)으로 구성하고 있기 때문에, VPS 등
     `F:\runo` 레이아웃이 없는 환경에서 빌드할 경우에는 git 의존으로의 전환이
     필요해진다. 실제로 VPS 에서 빌드할 단계가 되면 대응한다.
  5. SSE2 만 있는 CPU 용 구현이 없기 때문에, `open-raid-z` 는 SSE2 경로만
     자체 구현을 남기고 있다. SSE2 버전을 추가하면 그 부분도 위임할 수 있다.
  6. `gf_xor` 는 SIMD 화해도 스칼라 대비 1.12〜1.25 배밖에 나오지 않는다
     (메모리 대역 율속). 비일시적 스토어(`_mm256_stream_si256`)로 캐시 오염을
     피하는 최적화가 효과를 볼 가능성이 있으며, 큰 버퍼용으로 시도해 볼 가치가
     있다(미검증).
