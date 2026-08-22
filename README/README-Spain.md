> Original japonés / 日本語原文: [README.md](../README.md)

# open-cpu

**Biblioteca de detección de conjuntos de instrucciones de CPU y despacho en
tiempo de ejecución** (Rust), común a todo el ecosistema `aon-co-jp`.

Se creó para evitar que `open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda`
escriban cada uno su propio código duplicado de detección de características de CPU.

**No es un servicio residente (demonio).** Es un crate de biblioteca ordinario que
cada repositorio añade a `[dependencies]` en su `Cargo.toml` y enlaza dentro
del mismo proceso.

## Qué permite hacer

1. **Detección en tiempo de ejecución de las características de la CPU** —
   `open_cpu::detect()` devuelve `&'static CpuCapabilities`. Usa
   `std::is_x86_feature_detected!` y almacena en caché el resultado de la primera
   detección en un `OnceLock`, por lo que el coste de llamarlo repetidamente es casi nulo.
2. **Despacho en tiempo de ejecución de las operaciones GF(2^8) de RAID6** — según
   el resultado de la detección, elige en tiempo de ejecución entre las
   implementaciones escalar / PCLMULQDQ / AVX2 / AVX-512.

## Uso

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

### Lista de la API pública

| API | Contenido | Despacho |
|---|---|---|
| `detect() -> &'static CpuCapabilities` | Detección de características de CPU (caché con `OnceLock`) | — |
| `runtime_summary() -> String` | Resumen de una línea con el resultado de la detección y la implementación elegida | — |
| `selected_impl() -> GfImpl` | Implementación elegida para las operaciones GF | — |
| `gf_xor(dst, src)` | `dst ^= src` (paridad P) | AVX2 / escalar |
| `gf_mul_parity(dst, src, factor)` | `dst ^= src * factor` (paridad Q) | AVX-512 (opt-in) / AVX2 / PCLMULQDQ / escalar |
| `gf_mul_pow2_xor(acc, src, times)` | `acc = acc * 2^times ^ src` | AVX2 / escalar |
| `gf_mul2_xor` / `gf_mul4_xor` | Lo anterior con `times=1` / `times=2` | Ídem |
| `raid6_parity(stripes, p, q)` | Calcula P/Q en bloque mediante tabla de coeficientes | Conforme a lo anterior |
| `raid6_parity3(stripes, p, q, r)` | Calcula P/Q/R en bloque mediante el método de Horner | Conforme a lo anterior |
| `gf_mul(a, b) -> u8` | Multiplicación GF de 1 byte (`const fn`) | — |
| `gf_mul2_byte(b) -> u8` | Duplicación en GF de 1 byte (`const fn`) | — |
| `raid6_coeff(i) -> u8` | Coeficiente `g^i` de RAID6 (`g = 2`) | — |

También se publican las versiones `*_scalar` / `*_avx2` / `*_pclmul` / `*_avx512`
que invocan cada implementación explícitamente (para benchmarks y verificación
cruzada; las versiones SIMD son `unsafe`).

## Conjuntos de instrucciones detectados

| Conjunto de instrucciones | Detección | Uso en este crate |
|---|---|---|
| SSE2 | ✅ | Se usa como apoyo de la ruta PCLMULQDQ |
| SSSE3 | ✅ | `pshufb` (reducción de la ruta PCLMULQDQ) |
| PCLMULQDQ | ✅ | Multiplicación GF(2^8) (implementación con multiplicación sin acarreo) |
| AVX2 | ✅ | Multiplicación GF(2^8) (split-table con `vpshufb`, elegida por defecto) |
| AVX-512F / BW / VL | ✅ | Existe una ruta de multiplicación GF(2^8) (**ejecución no verificada**, véase más abajo) |
| POPCNT | ✅ | Solo detección (sin implementación que lo use) |
| BMI1 / BMI2 | ✅ | Solo detección (sin implementación que lo use) |
| FMA3 | ✅ | Solo detección (sin implementación que lo use) |
| AES-NI | ✅ | Solo detección (sin implementación que lo use) |
| SHA-NI | ✅ | Solo detección (sin implementación que lo use) |
| AVX-VNNI | ✅ | Solo detección (para futura inferencia de IA, sin implementación que lo use) |
| AVX-512 VNNI | ✅ | Solo detección (para futura inferencia de IA, sin implementación que lo use) |

En arquitecturas distintas de x86/x86_64 todos los campos son `false` y se recurre
a la implementación escalar (la compilación funciona).

## Detalles de la implementación GF(2^8)

El polinomio irreducible es `0x11d` (x^8+x^4+x^3+x^2+1) y el generador es `g = 2`.
Igual que en md/RAID6 de Linux y en ZFS RAID-Z.

- **Escalar**: implementación de referencia mediante tablas nibble split (16 entradas × 2).
- **PCLMULQDQ**: al expandir cada byte a intervalos de 16 bits, el producto sin acarreo
  con un coeficiente de 8 bits (15 bits como máximo) no desborda hacia la ranura contigua.
  Aprovechando esta propiedad se multiplican 4 bytes de una vez con una sola instrucción
  y se reduce a GF(2^8) con dos `pshufb`. 16 byte/iter.
- **AVX2**: implementación split-table con `vpshufb`. 32 byte/iter.
- **AVX-512F/BW**: procesa la misma split-table a 64 byte/iter.

## Benchmark medido

`cargo run --release --example bench` (4 MiB × 50 veces = 200 MiB, factor=0x8d)

Máquina de desarrollo: **AMD Ryzen 9 3950X** / Windows 11 / rustc 1.96.0

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

**Sobre la dispersión de los valores medidos (registro honesto)**: tras ejecutar
4 veces seguidas, el factor de aceleración AVX2 de la multiplicación GF varió en el
rango de **11,6 a 18,1 veces**, el método de Horner de **2,70 a 3,52 veces** y el
XOR de **1,12 a 1,25 veces** (el lado escalar se mantuvo estable en torno a 207 ms).
El lado SIMD alcanza entre 10.000 y 20.000 MiB/s, por lo que está **limitado por el
ancho de banda de memoria** y es sensible al estado de la caché y a otros procesos.
La tabla anterior recoge tal cual el resultado de una única ejecución; lo correcto
es entender los factores como rangos.

- **La multiplicación GF(2^8) por un coeficiente arbitrario es de 11,6 a 18,1 veces
  más rápida con AVX2 que en escalar (medido)**. Resulta eficaz en la paridad Q de
  RAID6 y en la ruta de recuperación.
- **El método de Horner es de 2,70 a 3,52 veces más rápido (medido)**. Como la versión
  escalar ya está optimizada con trucos de bits sobre u64, la diferencia no es tan
  grande como en la multiplicación GF.
- **El XOR simple es de 1,12 a 1,25 veces más rápido**. Como ya en escalar (u64) está
  pegado al ancho de banda de memoria, el margen de la vectorización SIMD es pequeño
  (como se esperaba).
- PCLMULQDQ da 1,90 veces y solo tiene sentido como **alternativa para CPU antiguas
  donde no se puede usar AVX2**.

## Estado de la verificación (divulgación honesta)

- ✅ **Escalar / PCLMULQDQ / AVX2**: verificados en ejecución en la máquina de
  desarrollo indicada. Mediante `cargo test` se comprueba la corrección de la
  implementación escalar tomando como referencia una implementación ingenua con
  desplazamientos de bits y, además, tomando la escalar como referencia, se comprueba
  la coincidencia de las salidas de la implementación PCLMULQDQ (**los 256 coeficientes
  posibles × 9 longitudes distintas**) y de la implementación AVX2 (8 coeficientes ×
  11 longitudes, incluido el tratamiento del resto). Pasan las 15 pruebas y los 2 doctests.
- ⚠️ **La ruta AVX-512 no está verificada en ejecución**. Como la máquina de desarrollo
  (Ryzen 9 3950X) no dispone de AVX-512, **solo se ha comprobado que compila**. Por
  seguridad no se selecciona en el despacho por defecto y únicamente se activa de forma
  opt-in cuando se define la variable de entorno `OPEN_CPU_ENABLE_AVX512=1`. Este
  tratamiento se mantendrá hasta que se verifique en una máquina con AVX-512.
- ⚠️ POPCNT/BMI1/BMI2/FMA/AES-NI/SHA-NI/VNNI **solo tienen campos de detección**;
  todavía no hay implementaciones de cálculo que los usen.

## Ejecución de pruebas y benchmarks

```
cargo build --release
cargo test --release
cargo run --release --example bench
```

## Adopciones (a fecha de 2026-08-22)

| Repositorio | Uso |
|---|---|
| [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z) | Se delega en este crate la detección de características de CPU, la multiplicación GF(2^8), el XOR y el método de Horner (ruta AVX2) de `zfs_accel_hlsl/src/simd.rs`. Tras la migración siguen pasando las 39 pruebas existentes y los valores numéricos coinciden por completo. |
| [`open-english`](https://github.com/aon-co-jp/open-english) | Resumen de una línea en el registro de arranque del servidor y `GET /v1/cpu-runtime` (devuelve en JSON el conjunto de instrucciones de CPU de la plataforma de ejecución). |

Sin adoptar (objetivos futuros): `aruaru-db` (sumas de verificación y compresión),
`aruaru-llm` (operaciones matriciales), `open-cuda` (alternativa por CPU cuando no hay GPU).

## Relacionado

- Procedimiento de migración: [PORTING.md](PORTING-Spain.md)
- Política de desarrollo y HANDOFF: [CLAUDE.md](CLAUDE-Spain.md)
- GitHub organization: https://github.com/aon-co-jp
