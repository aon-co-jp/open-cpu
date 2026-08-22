> Original japonés / 日本語原文: [CLAUDE.md](../CLAUDE.md)

# Política de desarrollo y reglas del entorno de desarrollo (open-cpu)

Este repositorio forma parte del ecosistema `aon-co-jp`. **La filosofía de diseño y
las reglas operativas comunes a todos los repositorios (verificación exhaustiva,
prohibición de informes exagerados, evitar reinventar la rueda, continuación
automática sin necesidad de confirmación, etc.) tienen como texto de referencia
[`open-raid-z/CLAUDE.md`](https://github.com/aon-co-jp/open-raid-z/blob/main/CLAUDE.md).**
No se reproducen aquí; en este documento solo se recogen los aspectos propios de
este repositorio.

La unidad de trabajo es `F:\runo\open-cpu` (nueva distribución).

## Papel de este repositorio

Se creó como base común porque `open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda`
iban a implementar **cada uno por su cuenta** la detección y el despacho en tiempo de
ejecución de los conjuntos de instrucciones de CPU
(AVX2/AVX-512/PCLMULQDQ/BMI1/BMI2/FMA3/AES-NI/POPCNT/SHA-NI).

- **Es un crate de biblioteca** (instrucción del usuario, 2026-08-22). No se convertirá
  en un servicio residente (demonio). Cada repositorio lo añade a `[dependencies]` y lo
  enlaza dentro del mismo proceso. No se debe introducir comunicación entre procesos.
- El alcance se limita a la «detección de características de CPU» y al «despacho de
  operaciones basado en ese resultado de detección». No se debe trasladar aquí la lógica
  de dominio de cada repositorio (estrategia de striping de RAID, gestión de páginas de
  la BD, carga de modelos del LLM, etc.).
- No aumentar los crates de los que se depende (actualmente `[dependencies]` está vacío,
  solo `std`). Al ser la base situada en la capa más baja de cada repositorio, introducir
  dependencias repercutiría en todo el conjunto.

## Reglas de implementación y verificación (propias de este repositorio)

- **Cuando se añada una implementación SIMD, hay que escribir a la vez, sin excepción,
  una prueba de coincidencia de salida con la implementación escalar.** Es obligatorio
  incluir en los casos de prueba longitudes no múltiplos del ancho del vector (restos).
- **Las rutas de código que no puedan ejecutarse en máquina real deben indicarse
  explícitamente como «no verificadas» y no deben seleccionarse en el despacho por
  defecto.** Actualmente esto se aplica a la ruta AVX-512 (la máquina de desarrollo,
  un AMD Ryzen 9 3950X, no dispone de AVX-512). Solo se activa mediante opt-in con
  `OPEN_CPU_ENABLE_AVX512=1`. En cuanto se complete la verificación en una máquina con
  AVX-512 habrá que retirar esta restricción y actualizar las menciones a «no verificado»
  en README.md / PORTING.md y en este archivo.
- Al escribir cifras de benchmark, calificar como «medido» **solo aquello que se haya
  medido realmente**. No presentar valores estimados o teóricos como si fueran medidos.
- No se usa `cargo bench`; basta con la medición sencilla mediante `std::time::Instant`
  en `examples/bench.rs` (para no aumentar las dependencias).

## Documentación multilingüe

En la carpeta `README/` se colocan las versiones en 15 idiomas de README / CLAUDE /
PORTING (práctica común del ecosistema, con la misma convención de nombres que
`open-raid-z` / `open-cuda`). **La versión japonesa (`README.md` / `CLAUDE.md` /
`PORTING.md` en la raíz del repositorio) es el texto de referencia**; cuando se
actualice su contenido hay que hacer que las versiones en 15 idiomas lo sigan
(no existe mecanismo de sincronización automática: se refleja a mano).

Idiomas: US English / UK English / Germany / Italy / France / Spain / Russia /
Ukraine / Hebrew / Persian (Iran) / Arabic / China / Taiwan / Korea / Japan.

## HANDOFF

- **2026-08-22 Creación inicial + integración en 2 repositorios**:
  se creó la implementación inicial a partir de un repositorio vacío.
  - `src/caps.rs`: estructura `CpuCapabilities` y `detect()` (caché con `OnceLock`).
    Los objetivos de detección son avx2 / avx512f / avx512bw / avx512vl / pclmulqdq /
    bmi1 / bmi2 / fma / aes / popcnt / sha / sse2 / ssse3 / avx-vnni / avx512vnni.
  - `src/gf.rs`: operaciones GF(2^8) de RAID6 (polinomio 0x11d, generador g=2).
    Se publican `gf_xor` / `gf_mul_parity` / `raid6_parity` / `raid6_coeff`, con
    despacho en tiempo de ejecución entre las 4 implementaciones escalar / PCLMULQDQ /
    AVX2 / AVX-512.
  - Medición (Ryzen 9 3950X, 4MiB×50 veces): scalar 1003 MiB/s, pclmulqdq 1916 MiB/s
    (1.91x), **avx2 22531 MiB/s (22.46x)**. AVX-512 no se pudo medir por no estar presente.
    → **Posteriormente se comprobó que este factor de avx2 varía en el rango de 11,6 a
    18,1 veces al volver a medir** (porque el lado SIMD está limitado por el ancho de
    banda de memoria; las cifras y rangos más recientes son los del apartado
    «Benchmark medido» de README.md).
  - `cargo test --release`: pasan las 11 pruebas y 1 doctest. Se ha confirmado que la
    implementación PCLMULQDQ coincide con la escalar para los 256 coeficientes.
  - Integraciones realizadas: incorporado mediante dependencia por path en `open-raid-z`
    (sustitución de las operaciones GF) y en `open-english/server` (endpoint
    `/v1/cpu-runtime` y registro de arranque).

- **2026-08-22 (continuación) Ciclo de mejora de la utilidad + función de presentación**:
  tras la implementación inicial se llevó a cabo un ciclo de desarrollo → TEST → corrección
  para mejorar la interoperabilidad y la utilidad práctica.
  - **Ciclo 1: adición de la API del método de Horner** (`gf_mul_pow2_xor` /
    `gf_mul2_xor` / `gf_mul4_xor`, AVX2 + escalar). Gracias a esto también se pudo
    delegar en open-cpu la ruta AVX2 de `mul2_xor_into` / `mul4_xor_into` de
    `open-raid-z`, reduciendo aún más el código x86 de aquel lado.
    También se añadió una ruta AVX2 a `gf_xor` (para que la delegación no supusiera una
    regresión de rendimiento).
  - **Ciclo 2: mejora de la comodidad de uso.** Se añadieron una implementación de
    `Display` y `has_all()` a `CpuCapabilities`. Se añadió `raid6_parity3()`, que calcula
    en bloque P/Q/R equivalentes a RAID-Z3. Se añadieron mediciones de XOR y del método
    de Horner a `examples/bench.rs`.
  - **Verificación**: `cargo test --release` **pasa las 15 pruebas y los 2 doctests**.
    El benchmark se ejecutó 4 veces seguidas y la dispersión (variación de 11,6 a 18,1
    veces, porque el lado SIMD está limitado por el ancho de banda de memoria) se
    registró honestamente como rango en el README.
  - **Un bug real encontrado al integrar en `open-english` (lección importante)**: la
    función de respuesta de presentación ante la pregunta «¿quién lo creó?» se
    implementó mediante coincidencia parcial de palabras clave (`"誰が作"`), y **al
    probarla en un navegador real se descubrió que no detectaba
    「誰が【このシステムを】作ったのですか?」** (porque entre el interrogativo y el verbo
    se intercalan otras palabras). Se corrigió separando la lista de interrogativos y la
    de verbos y evaluándolas con una condición AND, y se comprobaron 10 ejemplos
    positivos y 6 negativos, además de una verificación E2E en un navegador real. Queda
    constancia de que era **el tipo de bug que no se encuentra con «las pruebas unitarias
    pasan», sino solo llegando a «escribirlo realmente en el chat»**.

- **Qué hacer a continuación**:
  1. Integrarlo progresivamente también en las sumas de verificación y la compresión de
     `aruaru-db`, en las operaciones matriciales de `aruaru-llm` y en la alternativa por
     CPU de `open-cuda` (actualmente solo hay 2 casos: `open-raid-z` y `open-english`).
  2. Cuando se pueda conseguir o disponer de una máquina con AVX-512, verificar la ruta
     AVX-512 mediante medición real y retirar la restricción de opt-in.
  3. Para los conjuntos de instrucciones que solo se detectan y no tienen implementación
     (POPCNT/BMI/FMA/AES-NI/SHA-NI), añadir implementaciones de cálculo empezando por
     aquellas que realmente se necesiten en el lado dependiente. En particular, son
     candidatos CRC32C mediante PCLMULQDQ para las sumas de verificación de `aruaru-db`
     y el producto escalar int8 mediante AVX-VNNI/AVX-512 VNNI para `aruaru-llm`.
  4. Como está montado con dependencia por path (`path = "../open-cpu"`), si se compila
     en entornos sin la distribución `F:\runo` (VPS, etc.) habrá que cambiar a una
     dependencia git. Se abordará cuando llegue el momento de compilar realmente en el VPS.
  5. Como no hay implementación para CPU con solo SSE2, `open-raid-z` conserva su propia
     implementación únicamente para la ruta SSE2. Si se añade una versión SSE2, también
     esa parte podrá delegarse.
  6. `gf_xor`, aun vectorizado con SIMD, solo alcanza de 1,12 a 1,25 veces la velocidad
     escalar (limitado por el ancho de banda de memoria). Es posible que resulte eficaz
     una optimización con almacenamiento no temporal (`_mm256_stream_si256`) para evitar
     la contaminación de la caché, y merece la pena probarlo con búferes grandes
     (no verificado).
