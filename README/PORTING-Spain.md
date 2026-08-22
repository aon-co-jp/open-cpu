> Original japonés / 日本語原文: [PORTING.md](../PORTING.md)

# PORTING.md — Procedimiento para migrar desde otros repositorios a `open-cpu`

Procedimiento de migración para concentrar en `open-cpu` el código de detección de
características de CPU y el código de operaciones GF(2^8) que `open-raid-z` /
`aruaru-db` / `aruaru-llm` / `open-cuda` y otros mantienen por separado.

## 0. Premisas

`open-cpu` es un **crate de biblioteca**, no un servicio residente. No hace falta
comunicación entre procesos ni lanzar otro proceso: basta con añadir la dependencia
en `Cargo.toml` y llamar a las funciones.

## 1. Añadir la dependencia

Bajo la unidad de trabajo local `F:\runo`, la dependencia por path es lo más sencillo:

```toml
[dependencies]
open-cpu = { path = "../open-cpu" }
```

Si se usa desde un crate miembro dentro de un workspace, lo recomendable es escribir en
el `Cargo.toml` de la raíz del workspace

```toml
[workspace.dependencies]
open-cpu = { path = "../open-cpu" }
```

y poner `open-cpu = { workspace = true }` en el miembro.

Si en el futuro se cambia a una dependencia git:

```toml
open-cpu = { git = "https://github.com/aon-co-jp/open-cpu", branch = "main" }
```

El nombre del crate lleva guion, `open-cpu`, y la ruta con la que se referencia desde
Rust lleva guion bajo, `open_cpu`.

## 2. Sustitución del código de detección de características de CPU

Antes de la migración (patrón habitual en cada repositorio):

```rust
static HAS_AVX2: OnceLock<bool> = OnceLock::new();
fn has_avx2() -> bool {
    *HAS_AVX2.get_or_init(|| is_x86_feature_detected!("avx2"))
}
```

Después de la migración:

```rust
if open_cpu::detect().avx2 { /* ... */ }
```

`detect()` ya está cacheado internamente con `OnceLock`, así que no hace falta volver a
cachear en el lado que lo llama. Como devuelve `&'static CpuCapabilities`, tampoco se
produce ninguna asignación de memoria.

Campos disponibles: `avx2` `avx512f` `avx512bw` `avx512vl` `pclmulqdq`
`bmi1` `bmi2` `fma` `aes` `popcnt` `sha` `sse2` `ssse3` `avx_vnni` `avx512vnni`.

## 3. Sustitución de las operaciones GF(2^8) / de paridad (principalmente `open-raid-z`)

El polinomio irreducible de `open-cpu` es `0x11d` y el generador es `g = 2`, idénticos a
los de md/RAID6 de Linux y ZFS RAID-Z. **Antes de migrar hay que comprobar sin falta si
el polinomio y el generador del propio repositorio coinciden con estos.** Si difieren,
los valores no cuadrarán.

| Forma habitual antes de la migración | Después de la migración |
|---|---|
| `for i in .. { p[i] ^= d[i] }` | `open_cpu::gf_xor(&mut p, &d)` |
| `for i in .. { q[i] ^= gf_mul(d[i], c) }` | `open_cpu::gf_mul_parity(&mut q, &d, c)` |
| `for i in .. { acc[i] = mul2(acc[i]) ^ d[i] }` | `open_cpu::gf_mul2_xor(&mut acc, &d)` |
| `for i in .. { acc[i] = mul4(acc[i]) ^ d[i] }` | `open_cpu::gf_mul4_xor(&mut acc, &d)` |
| Método de Horner con `×2^n` un número arbitrario de veces | `open_cpu::gf_mul_pow2_xor(&mut acc, &d, n)` |
| `gf_mul(a: u8, b: u8) -> u8` propio | `open_cpu::gf_mul(a, b)` (`const fn`) |
| `mul2_byte(b) -> u8` propio | `open_cpu::gf_mul2_byte(b)` (`const fn`) |
| Cálculo propio del coeficiente `g^i` | `open_cpu::raid6_coeff(i)` |
| Cálculo en bloque de P/Q | `open_cpu::raid6_parity(&stripes, &mut p, &mut q)` |
| Cálculo en bloque de P/Q/R (equivalente a RAID-Z3) | `open_cpu::raid6_parity3(&stripes, &mut p, &mut q, &mut r)` |

`gf_mul_parity` / `gf_xor` provocan panic si `dst.len() != src.len()`.
Hay que igualar las longitudes de stripe en el lado que llama.

Si se quiere invocar explícitamente una implementación concreta (para benchmarks o
verificación):

- `open_cpu::gf_mul_parity_scalar(...)` / `gf_xor_scalar(...)` /
  `gf_mul_pow2_xor_scalar(...)` — safe
- `unsafe { open_cpu::gf_mul_parity_avx2(...) }` — quien llama garantiza el soporte de AVX2
- `unsafe { open_cpu::gf_mul_parity_pclmul(...) }` — ídem para SSSE3+PCLMULQDQ
- `unsafe { open_cpu::gf_mul_parity_avx512(...) }` — **ejecución no verificada**

## 4. Verificación posterior a la migración (obligatoria)

1. `cargo build` — que la resolución de dependencias y la compilación funcionen.
2. `cargo test` — **que pasen todas las pruebas existentes**. En particular, comprobar
   que las secuencias de bytes de la paridad coinciden exactamente antes y después de la
   sustitución. Si las pruebas existentes no comparan valores de paridad, hay que
   añadirlo durante la migración.
3. Si se emite una línea con `open_cpu::runtime_summary()` en el registro, después se
   podrá comprobar qué implementación se seleccionó en la máquina real.

## 4.5 Ejemplo real de migración (`open-raid-z`, 2026-08-22)

Como referencia, estos son los puntos clave del primer caso real de migración:

- Se sustituyó `std::is_x86_feature_detected!` dentro de `detect_level()` por una
  referencia a `open_cpu::detect()` (se mantuvo tal cual el enum propio del repositorio
  `SimdLevel` y solo se trasladó a open-cpu **el material con el que se decide**). Así no
  hubo que modificar en absoluto el código que lo llamaba.
- Solo se delegó en open-cpu **la ruta AVX2** de `gf_mul_xor_into()` / `xor_into()` /
  `mul_pow2_xor_into()`; la ruta AVX-512 (no verificada también del lado de open-cpu) y
  la ruta SSE2 (sin implementación en open-cpu) conservaron la implementación del
  repositorio. La decisión fue: **no hacer sustituciones que empeoren el rendimiento**.
- Los kernels SIMD que dejaron de usarse no se eliminaron, sino que se dejaron con
  `#[allow(dead_code)]` como referencia para una futura verificación cruzada con el lado
  de open-cpu.

Se recomienda esta forma de proceder: «no sustituirlo todo de golpe, sino delegar por
etapas empezando por las partes equivalentes y que no pierdan rendimiento».

## 5. Advertencias

- **La ruta AVX-512 no está verificada en ejecución** (la máquina de desarrollo no
  dispone de ella). Como no se selecciona por defecto, la migración no altera el
  comportamiento. Solo hay que definir `OPEN_CPU_ENABLE_AVX512=1` cuando se vaya a
  verificar en una máquina con AVX-512.
- POPCNT/BMI/FMA/AES-NI/SHA-NI/VNNI son **solo detección**. Como en `open-cpu` todavía no
  hay implementaciones de sumas de verificación, compresión ni operaciones matriciales
  que los usen, el código correspondiente de `aruaru-db` / `aruaru-llm` aún no se puede
  migrar (sí es posible migrar antes solo la parte de detección).
- Fuera de x86 (ARM, etc.) todas las características son `false` y se recurre a la
  implementación escalar. La compilación cruzada funciona, pero no hay soporte para
  optimizaciones como NEON.
