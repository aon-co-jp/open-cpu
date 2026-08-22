> Japanisches Original / 日本語原文: [PORTING.md](../PORTING.md)

# PORTING.md — Vorgehen zum Umstieg anderer Repositories auf `open-cpu`

Migrationsanleitung, um den in `open-raid-z` / `aruaru-db` / `aruaru-llm` /
`open-cuda` usw. jeweils einzeln vorhandenen Code zur CPU-Feature-Erkennung und für
GF(2^8)-Operationen in `open-cpu` zusammenzuführen.

## 0. Voraussetzungen

`open-cpu` ist eine **Bibliotheks-Crate** und kein residenter Dienst.
Interprozesskommunikation oder das Starten eines separaten Prozesses sind nicht nötig;
es genügt, die Abhängigkeit in `Cargo.toml` einzutragen und die Funktionen aufzurufen.

## 1. Hinzufügen der Abhängigkeit

Unterhalb des lokalen Arbeitslaufwerks `F:\runo` ist die path-Abhängigkeit am
einfachsten:

```toml
[dependencies]
open-cpu = { path = "../open-cpu" }
```

Bei Verwendung aus einer Member-Crate innerhalb eines Workspace ist es empfehlenswert,
in der `Cargo.toml` des Workspace-Roots

```toml
[workspace.dependencies]
open-cpu = { path = "../open-cpu" }
```

zu schreiben und auf der Member-Seite `open-cpu = { workspace = true }` zu verwenden.

Falls künftig auf eine git-Abhängigkeit umgestellt wird:

```toml
open-cpu = { git = "https://github.com/aon-co-jp/open-cpu", branch = "main" }
```

Der Crate-Name lautet mit Bindestrich `open-cpu`, der Pfad für die Referenzierung aus
Rust mit Unterstrich `open_cpu`.

## 2. Ersetzen des Codes zur CPU-Feature-Erkennung

Vor der Migration (ein in vielen Repositories übliches Muster):

```rust
static HAS_AVX2: OnceLock<bool> = OnceLock::new();
fn has_avx2() -> bool {
    *HAS_AVX2.get_or_init(|| is_x86_feature_detected!("avx2"))
}
```

Nach der Migration:

```rust
if open_cpu::detect().avx2 { /* ... */ }
```

`detect()` ist intern bereits per `OnceLock` gecacht, daher ist auf der Aufruferseite
kein zusätzliches Caching nötig. Da `&'static CpuCapabilities` zurückgegeben wird,
entsteht auch keine Allokation.

Verfügbare Felder: `avx2` `avx512f` `avx512bw` `avx512vl` `pclmulqdq`
`bmi1` `bmi2` `fma` `aes` `popcnt` `sha` `sse2` `ssse3` `avx_vnni` `avx512vnni`.

## 3. Ersetzen der GF(2^8)-/Paritätsoperationen (hauptsächlich `open-raid-z`)

Das irreduzible Polynom von `open-cpu` ist `0x11d`, das erzeugende Element `g = 2`,
identisch mit Linux md/RAID6 sowie ZFS RAID-Z. **Vor der Migration ist unbedingt zu
prüfen, ob Polynom und erzeugendes Element der eigenen Repository damit
übereinstimmen.** Andernfalls stimmen die Zahlenwerte nicht mehr überein.

| Vor der Migration übliche Form | Nach der Migration |
|---|---|
| `for i in .. { p[i] ^= d[i] }` | `open_cpu::gf_xor(&mut p, &d)` |
| `for i in .. { q[i] ^= gf_mul(d[i], c) }` | `open_cpu::gf_mul_parity(&mut q, &d, c)` |
| `for i in .. { acc[i] = mul2(acc[i]) ^ d[i] }` | `open_cpu::gf_mul2_xor(&mut acc, &d)` |
| `for i in .. { acc[i] = mul4(acc[i]) ^ d[i] }` | `open_cpu::gf_mul4_xor(&mut acc, &d)` |
| Horner-Schema mit beliebig häufigem `×2^n` | `open_cpu::gf_mul_pow2_xor(&mut acc, &d, n)` |
| Eigenes `gf_mul(a: u8, b: u8) -> u8` | `open_cpu::gf_mul(a, b)` (`const fn`) |
| Eigenes `mul2_byte(b) -> u8` | `open_cpu::gf_mul2_byte(b)` (`const fn`) |
| Eigene Berechnung der `g^i`-Koeffizienten | `open_cpu::raid6_coeff(i)` |
| Gesammelte P/Q-Berechnung | `open_cpu::raid6_parity(&stripes, &mut p, &mut q)` |
| Gesammelte P/Q/R-Berechnung (entsprechend RAID-Z3) | `open_cpu::raid6_parity3(&stripes, &mut p, &mut q, &mut r)` |

`gf_mul_parity` / `gf_xor` lösen bei `dst.len() != src.len()` eine Panic aus.
Die Stripe-Längen sind auf der Aufruferseite anzugleichen.

Wenn eine bestimmte Implementierung explizit aufgerufen werden soll (zu Benchmark-
oder Verifizierungszwecken):

- `open_cpu::gf_mul_parity_scalar(...)` / `gf_xor_scalar(...)` /
  `gf_mul_pow2_xor_scalar(...)` — safe
- `unsafe { open_cpu::gf_mul_parity_avx2(...) }` — der Aufrufer garantiert
  AVX2-Unterstützung
- `unsafe { open_cpu::gf_mul_parity_pclmul(...) }` — ebenso für SSSE3+PCLMULQDQ
- `unsafe { open_cpu::gf_mul_parity_avx512(...) }` — **Ausführung nicht verifiziert**

## 4. Verifizierung nach der Migration (verpflichtend)

1. `cargo build` — Auflösung der Abhängigkeiten und Build müssen gelingen.
2. `cargo test` — **alle bestehenden Tests müssen bestehen**. Insbesondere ist zu
   prüfen, dass die Byte-Folgen der Parität vor und nach der Ersetzung exakt
   übereinstimmen. Enthalten die bestehenden Tests keinen Vergleich der Paritätswerte,
   ist er bei der Migration zu ergänzen.
3. Gibt man `open_cpu::runtime_summary()` als eine Zeile ins Log aus, lässt sich später
   nachvollziehen, welche Implementierung auf der realen Maschine gewählt wurde.

## 4.5 Konkretes Migrationsbeispiel (`open-raid-z`, 2026-08-22)

Zur Orientierung die Kernpunkte des ersten realen Migrationsfalls:

- Das `std::is_x86_feature_detected!` in `detect_level()` wurde durch eine Referenz auf
  `open_cpu::detect()` ersetzt (der repository-spezifische Aufzählungstyp `SimdLevel`
  blieb unverändert erhalten, und **nur die Entscheidungsgrundlage dafür** wurde nach
  open-cpu verlagert). Die bestehenden Aufrufstellen mussten überhaupt nicht geändert
  werden.
- Von `gf_mul_xor_into()` / `xor_into()` / `mul_pow2_xor_into()` wurde **nur der
  AVX2-Pfad** an open-cpu delegiert; der AVX-512-Pfad (auch auf open-cpu-Seite nicht
  verifiziert) und der SSE2-Pfad (in open-cpu nicht implementiert) blieben als
  Implementierung der Repository erhalten. Die Entscheidung lautete: **keine Ersetzung,
  die die Performance senkt.**
- Nicht mehr verwendete SIMD-Kernel wurden nicht gelöscht, sondern mit
  `#[allow(dead_code)]` versehen belassen und dienen als Referenz für eine künftige
  Gegenprüfung mit der open-cpu-Seite.

Empfohlen wird dieses Vorgehen, „nicht alles auf einmal zu ersetzen, sondern schrittweise
dort zu delegieren, wo es äquivalent ist und die Performance nicht sinkt“.

## 5. Hinweise

- **Der AVX-512-Pfad ist in der Ausführung nicht verifiziert** (der Entwicklungsrechner
  besitzt es nicht). Da er standardmäßig nicht gewählt wird, ändert sich das Verhalten
  durch die Migration nicht. Nur zur Verifizierung auf einer AVX-512-Maschine ist
  `OPEN_CPU_ENABLE_AVX512=1` zu setzen.
- POPCNT/BMI/FMA/AES-NI/SHA-NI/VNNI werden **nur erkannt**. Implementierungen für
  Prüfsummen, Kompression oder Matrixoperationen, die diese nutzen, gibt es in
  `open-cpu` noch nicht, daher lässt sich der betreffende Code auf Seiten von
  `aruaru-db` / `aruaru-llm` noch nicht migrieren (nur den Erkennungsteil vorab zu
  migrieren, ist möglich).
- Außerhalb von x86 (z. B. ARM) sind alle Features `false`, und es wird auf die
  Skalarimplementierung zurückgefallen. Der Cross-Build gelingt, Optimierungen wie NEON
  werden jedoch nicht unterstützt.
