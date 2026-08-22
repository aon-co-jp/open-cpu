> Japanisches Original / 日本語原文: [README.md](../README.md)

# open-cpu

Gemeinsame **Bibliothek zur Erkennung von CPU-Befehlssätzen und zum Laufzeit-Dispatch**
(Rust) für das `aon-co-jp`-Ökosystem.

Sie wurde neu geschaffen, um zu vermeiden, dass `open-raid-z` / `aruaru-db` /
`aruaru-llm` / `open-cuda` jeweils eigenen, redundanten Code zur CPU-Feature-Erkennung
schreiben.

**Es handelt sich nicht um einen residenten Dienst (Daemon).** Es ist eine gewöhnliche
Bibliotheks-Crate, die jede Repository unter `[dependencies]` in der `Cargo.toml`
einträgt und innerhalb desselben Prozesses linkt.

## Funktionsumfang

1. **Laufzeit-Erkennung von CPU-Features** — `open_cpu::detect()` liefert
   `&'static CpuCapabilities`. Es verwendet `std::is_x86_feature_detected!` und
   speichert das Ergebnis der ersten Erkennung in einem `OnceLock`, sodass beliebig
   häufige Aufrufe nahezu keine Kosten verursachen.
2. **Laufzeit-Dispatch der RAID6-GF(2^8)-Operationen** — abhängig vom Erkennungsergebnis
   wird zur Laufzeit die Skalar- / PCLMULQDQ- / AVX2- / AVX-512-Implementierung gewählt.

## Verwendung

```toml
# Cargo.toml der abhängigen Seite
[dependencies]
open-cpu = { path = "../open-cpu" }
```

```rust
// 1. CPU-Feature-Erkennung
let caps = open_cpu::detect();
println!("{}", caps);   // Display-Implementierung vorhanden
// => sse2 ssse3 popcnt aes pclmulqdq bmi1 bmi2 fma sha avx2

if caps.avx2 { /* ... */ }
if caps.has_all(&[caps.avx2, caps.fma]) { /* Pfad für AVX2+FMA3 */ }

// 2. RAID6-Parität (Verfahren mit Koeffiziententabelle)
let d0 = vec![1u8; 4096];
let d1 = vec![2u8; 4096];
let mut p = vec![0u8; 4096];
let mut q = vec![0u8; 4096];
open_cpu::raid6_parity(&[&d0, &d1], &mut p, &mut q);

// 3. P/Q/R entsprechend RAID-Z3 (Horner-Schema, schnell und ohne Koeffiziententabelle)
let mut r = vec![0u8; 4096];
open_cpu::raid6_parity3(&[&d0, &d1], &mut p, &mut q, &mut r);

// Einzelne APIs
open_cpu::gf_xor(&mut p, &d0);                 // p ^= d0          (P-Parität)
open_cpu::gf_mul_parity(&mut q, &d0, 0x02);    // q ^= d0 * 0x02   (Q-Parität)
open_cpu::gf_mul2_xor(&mut q, &d0);            // q = q*2 ^ d0     (Horner-Schema)
open_cpu::gf_mul4_xor(&mut r, &d0);            // r = r*4 ^ d0     (Horner-Schema)

// Einzeilige Zusammenfassung für das Log
println!("{}", open_cpu::runtime_summary());
// => open-cpu 0.1.0 | features: ... | gf impl: Avx2
```

### Übersicht der öffentlichen API

| API | Inhalt | Dispatch |
|---|---|---|
| `detect() -> &'static CpuCapabilities` | CPU-Feature-Erkennung (`OnceLock`-Cache) | — |
| `runtime_summary() -> String` | Einzeilige Zusammenfassung aus Erkennungsergebnis + gewählter Implementierung | — |
| `selected_impl() -> GfImpl` | Für die GF-Operationen gewählte Implementierung | — |
| `gf_xor(dst, src)` | `dst ^= src` (P-Parität) | AVX2 / Skalar |
| `gf_mul_parity(dst, src, factor)` | `dst ^= src * factor` (Q-Parität) | AVX-512 (opt-in) / AVX2 / PCLMULQDQ / Skalar |
| `gf_mul_pow2_xor(acc, src, times)` | `acc = acc * 2^times ^ src` | AVX2 / Skalar |
| `gf_mul2_xor` / `gf_mul4_xor` | Obiges mit `times=1` / `times=2` | wie oben |
| `raid6_parity(stripes, p, q)` | Berechnet P/Q gesammelt per Koeffiziententabelle | entsprechend obigem |
| `raid6_parity3(stripes, p, q, r)` | Berechnet P/Q/R gesammelt per Horner-Schema | entsprechend obigem |
| `gf_mul(a, b) -> u8` | GF-Multiplikation eines Bytes (`const fn`) | — |
| `gf_mul2_byte(b) -> u8` | Verdopplung eines Bytes über GF (`const fn`) | — |
| `raid6_coeff(i) -> u8` | RAID6-Koeffizient `g^i` (`g = 2`) | — |

Zusätzlich sind die Varianten `*_scalar` / `*_avx2` / `*_pclmul` / `*_avx512`
zum expliziten Aufruf der jeweiligen Implementierung öffentlich
(für Benchmarks und Gegenprüfungen; die SIMD-Versionen sind `unsafe`).

## Erkannte Befehlssätze

| Befehlssatz | Erkennung | Nutzung in dieser Crate |
|---|---|---|
| SSE2 | ✅ | Als Unterstützung für den PCLMULQDQ-Pfad verwendet |
| SSSE3 | ✅ | `pshufb` (Reduktion im PCLMULQDQ-Pfad) |
| PCLMULQDQ | ✅ | GF(2^8)-Multiplikation (Implementierung per carry-less multiplication) |
| AVX2 | ✅ | GF(2^8)-Multiplikation (`vpshufb` split-table, standardmäßig gewählt) |
| AVX-512F / BW / VL | ✅ | GF(2^8)-Multiplikationspfad vorhanden (**Ausführung nicht verifiziert**, siehe unten) |
| POPCNT | ✅ | Nur Erkennung (keine nutzende Implementierung) |
| BMI1 / BMI2 | ✅ | Nur Erkennung (keine nutzende Implementierung) |
| FMA3 | ✅ | Nur Erkennung (keine nutzende Implementierung) |
| AES-NI | ✅ | Nur Erkennung (keine nutzende Implementierung) |
| SHA-NI | ✅ | Nur Erkennung (keine nutzende Implementierung) |
| AVX-VNNI | ✅ | Nur Erkennung (für künftige KI-Inferenz, keine nutzende Implementierung) |
| AVX-512 VNNI | ✅ | Nur Erkennung (für künftige KI-Inferenz, keine nutzende Implementierung) |

Auf anderen Architekturen als x86/x86_64 sind sämtliche Felder `false`, und es wird
auf die Skalarimplementierung zurückgefallen (der Build gelingt).

## Details der GF(2^8)-Implementierung

Das irreduzible Polynom ist `0x11d` (x^8+x^4+x^3+x^2+1), das erzeugende Element `g = 2`.
Identisch mit Linux md/RAID6 sowie ZFS RAID-Z.

- **Skalar**: Referenzimplementierung mittels nibble-split-Tabelle (16 Einträge × 2).
- **PCLMULQDQ**: Spreizt man jedes Byte in 16-bit-Abständen, so kann das carry-less
  Produkt mit einem 8-bit-Koeffizienten (maximal 15 bit) nicht in den benachbarten
  Slot überlaufen. Diese Eigenschaft wird genutzt, um mit einer Instruktion 4 Bytes
  gemeinsam zu multiplizieren und anschließend mit zweimal `pshufb` nach GF(2^8) zu
  reduzieren. 16 byte/iter.
- **AVX2**: split-table-Implementierung mit `vpshufb`. 32 byte/iter.
- **AVX-512F/BW**: verarbeitet dieselbe split-table mit 64 byte/iter.

## Gemessene Benchmarks

`cargo run --release --example bench` (4 MiB × 50 Durchläufe = 200 MiB, factor=0x8d)

Entwicklungsrechner: **AMD Ryzen 9 3950X** / Windows 11 / rustc 1.96.0

```
open-cpu 0.1.0 | features: sse2 ssse3 popcnt aes pclmulqdq bmi1 bmi2 fma sha avx2 | gf impl: Avx2
scalar   :  207.5 ms     963 MiB/s
pclmulqdq:  109.0 ms    1835 MiB/s  (1.90x vs scalar)
avx2     :   11.4 ms   17473 MiB/s  (18.14x vs scalar)
avx512   : Auf dieser CPU nicht vorhanden, daher keine Messung möglich (nicht verifiziert)

xor scalar     :  11.5 ms   17451 MiB/s
xor dispatch   :   9.7 ms   20616 MiB/s  (1.15〜1.25x vs scalar)

horner scalar  :  51.5 ms    3880 MiB/s
horner dispatch:  14.8 ms   13521 MiB/s  (2.70〜3.52x vs scalar)
```

**Zur Streuung der Messwerte (ehrliche Aufzeichnung)**: Bei vier aufeinanderfolgenden
Durchläufen schwankte der AVX2-Faktor der GF-Multiplikation im Bereich von
**11,6–18,1×**, das Horner-Schema von **2,70–3,52×** und XOR von **1,12–1,25×**
(die Skalarseite blieb stabil bei etwa 207 ms). Da die SIMD-Seite 10.000–20.000 MiB/s
erreicht und damit **durch die Speicherbandbreite begrenzt** ist, wirken sich
Cache-Zustand und andere Prozesse leicht aus. Die obige Tabelle gibt das Ergebnis
eines einzelnen Laufs unverändert wieder; die Faktoren sind korrekterweise als
Bereiche zu verstehen.

- **Die GF(2^8)-Multiplikation mit beliebigem Koeffizienten ist mit AVX2 um den
  Faktor 11,6–18,1 gegenüber Skalar schneller (gemessen)**. Das wirkt sich auf den
  Q-Paritäts- und den Wiederherstellungspfad von RAID6 aus.
- **Das Horner-Schema erreicht Faktor 2,70–3,52 (gemessen)**. Da die Skalarversion
  bereits mit u64-Bit-Tricks optimiert ist, fällt der Unterschied geringer aus als
  bei der GF-Multiplikation.
- **Einfaches XOR erreicht Faktor 1,12–1,25**. Weil bereits die Skalarvariante (u64)
  an der Speicherbandbreite klebt, ist der Spielraum für eine SIMD-Umsetzung gering
  (wie erwartet).
- PCLMULQDQ liegt bei Faktor 1,90 und ist nur als **Fallback für ältere CPUs ohne
  AVX2** sinnvoll.

## Verifizierungsstand (ehrliche Offenlegung)

- ✅ **Skalar / PCLMULQDQ / AVX2**: Auf dem oben genannten Entwicklungsrechner
  ausführungsgeprüft. In `cargo test` wird die Korrektheit der Skalarimplementierung
  gegen eine naive Bitshift-Implementierung geprüft und darüber hinaus, ausgehend von
  der Skalarimplementierung, die Übereinstimmung der Ausgaben der PCLMULQDQ-Implementierung
  (**alle 256 Koeffizienten × 9 Längen**) und der AVX2-Implementierung (8 Koeffizienten ×
  11 Längen, einschließlich Restbehandlung) bestätigt. Alle 15 Tests + 2 doctests
  bestehen.
- ⚠️ **Der AVX-512-Pfad ist in der Ausführung nicht verifiziert.** Da der
  Entwicklungsrechner (Ryzen 9 3950X) kein AVX-512 besitzt, ist **nur bestätigt, dass
  er sich kompilieren lässt**. Aus Sicherheitsgründen wird er im Standard-Dispatch
  nicht gewählt und ist nur dann per opt-in aktiv, wenn die Umgebungsvariable
  `OPEN_CPU_ENABLE_AVX512=1` gesetzt ist. Diese Behandlung bleibt bestehen, bis die
  Verifizierung auf einem Rechner mit AVX-512 abgeschlossen ist.
- ⚠️ Für POPCNT/BMI1/BMI2/FMA/AES-NI/SHA-NI/VNNI existieren **lediglich
  Erkennungsfelder**; Rechenimplementierungen, die diese nutzen, gibt es noch nicht.

## Ausführen von Tests und Benchmarks

```
cargo build --release
cargo test --release
cargo run --release --example bench
```

## Einsatzstand (Stand 2026-08-22)

| Repository | Verwendung |
|---|---|
| [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z) | CPU-Feature-Erkennung, GF(2^8)-Multiplikation, XOR und Horner-Schema (AVX2-Pfad) in `zfs_accel_hlsl/src/simd.rs` an diese Crate delegiert. Auch nach der Migration bestehen alle bisherigen 39 Tests, und die Zahlenwerte stimmen exakt überein. |
| [`open-english`](https://github.com/aon-co-jp/open-english) | Einzeilige Zusammenfassung im Serverstart-Log sowie `GET /v1/cpu-runtime` (liefert die CPU-Befehlssätze der Ausführungsplattform als JSON). |

Noch nicht eingeführt (künftige Ziele): `aruaru-db` (Prüfsummen, Kompression),
`aruaru-llm` (Matrixoperationen), `open-cuda` (CPU-Fallback bei fehlender GPU).

## Verwandtes

- Migrationsanleitung: [PORTING.md](PORTING-Germany.md)
- Entwicklungsrichtlinien und HANDOFF: [CLAUDE.md](CLAUDE-Germany.md)
- GitHub organization: https://github.com/aon-co-jp
