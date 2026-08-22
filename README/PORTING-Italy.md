> Originale giapponese / 日本語原文: [PORTING.md](../PORTING.md)

# PORTING.md — procedura per passare da altri repository a `open-cpu`

Procedura di migrazione per accentrare in `open-cpu` il codice di rilevamento delle
funzionalità della CPU e di calcolo GF(2^8) che `open-raid-z` / `aruaru-db` /
`aruaru-llm` / `open-cuda` e altri possiedono singolarmente.

## 0. Premessa

`open-cpu` è un **crate di libreria**, non un servizio residente. Non servono
comunicazioni tra processi né l'avvio di processi separati: basta aggiungere la
dipendenza al `Cargo.toml` e chiamare le funzioni.

## 1. Aggiunta della dipendenza

Sotto il drive di lavoro locale `F:\runo` la dipendenza path è la più semplice:

```toml
[dependencies]
open-cpu = { path = "../open-cpu" }
```

Se lo si usa da un crate membro all'interno di un workspace, è preferibile scrivere
nel `Cargo.toml` della radice del workspace

```toml
[workspace.dependencies]
open-cpu = { path = "../open-cpu" }
```

e indicare sul lato membro `open-cpu = { workspace = true }`.

Se in futuro si passa a una dipendenza git:

```toml
open-cpu = { git = "https://github.com/aon-co-jp/open-cpu", branch = "main" }
```

Il nome del crate è con trattino, `open-cpu`; il percorso con cui lo si referenzia da
Rust è con underscore, `open_cpu`.

## 2. Sostituzione del codice di rilevamento delle funzionalità della CPU

Prima della migrazione (schema frequente nei vari repository):

```rust
static HAS_AVX2: OnceLock<bool> = OnceLock::new();
fn has_avx2() -> bool {
    *HAS_AVX2.get_or_init(|| is_x86_feature_detected!("avx2"))
}
```

Dopo la migrazione:

```rust
if open_cpu::detect().avx2 { /* ... */ }
```

`detect()` è già memorizzato in cache internamente con `OnceLock`, quindi non è
necessario che il chiamante lo memorizzi di nuovo. Poiché restituisce
`&'static CpuCapabilities`, non si verificano nemmeno allocazioni.

Campi disponibili: `avx2` `avx512f` `avx512bw` `avx512vl` `pclmulqdq`
`bmi1` `bmi2` `fma` `aes` `popcnt` `sha` `sse2` `ssse3` `avx_vnni` `avx512vnni`.

## 3. Sostituzione dei calcoli GF(2^8) / di parità (soprattutto `open-raid-z`)

Il polinomio irriducibile di `open-cpu` è `0x11d` e il generatore è `g = 2`, gli stessi
di Linux md/RAID6 e di ZFS RAID-Z. **Prima della migrazione, verificare
obbligatoriamente che il polinomio e il generatore del proprio repository coincidano
con questi.** In caso contrario i valori numerici non torneranno.

| Forma frequente prima della migrazione | Dopo la migrazione |
|---|---|
| `for i in .. { p[i] ^= d[i] }` | `open_cpu::gf_xor(&mut p, &d)` |
| `for i in .. { q[i] ^= gf_mul(d[i], c) }` | `open_cpu::gf_mul_parity(&mut q, &d, c)` |
| `for i in .. { acc[i] = mul2(acc[i]) ^ d[i] }` | `open_cpu::gf_mul2_xor(&mut acc, &d)` |
| `for i in .. { acc[i] = mul4(acc[i]) ^ d[i] }` | `open_cpu::gf_mul4_xor(&mut acc, &d)` |
| Metodo di Horner con `×2^n` un numero arbitrario di volte | `open_cpu::gf_mul_pow2_xor(&mut acc, &d, n)` |
| `gf_mul(a: u8, b: u8) -> u8` proprietario | `open_cpu::gf_mul(a, b)` (`const fn`) |
| `mul2_byte(b) -> u8` proprietario | `open_cpu::gf_mul2_byte(b)` (`const fn`) |
| Calcolo proprietario del coefficiente `g^i` | `open_cpu::raid6_coeff(i)` |
| Calcolo cumulativo di P/Q | `open_cpu::raid6_parity(&stripes, &mut p, &mut q)` |
| Calcolo cumulativo di P/Q/R (equivalente a RAID-Z3) | `open_cpu::raid6_parity3(&stripes, &mut p, &mut q, &mut r)` |

`gf_mul_parity` / `gf_xor` vanno in panic se `dst.len() != src.len()`.
Il chiamante deve allineare le lunghezze delle stripe.

Se si vuole invocare esplicitamente una specifica implementazione (a scopo di
benchmark o verifica):

- `open_cpu::gf_mul_parity_scalar(...)` / `gf_xor_scalar(...)` /
  `gf_mul_pow2_xor_scalar(...)` — safe
- `unsafe { open_cpu::gf_mul_parity_avx2(...) }` — il chiamante garantisce il supporto AVX2
- `unsafe { open_cpu::gf_mul_parity_pclmul(...) }` — idem per SSSE3+PCLMULQDQ
- `unsafe { open_cpu::gf_mul_parity_avx512(...) }` — **esecuzione non verificata**

## 4. Verifica dopo la migrazione (obbligatoria)

1. `cargo build` — la risoluzione delle dipendenze e la compilazione devono riuscire.
2. `cargo test` — **tutti i test esistenti devono passare**. In particolare, verificare
   che la sequenza di byte della parità coincida perfettamente prima e dopo la
   sostituzione. Se nei test esistenti non c'è un confronto dei valori di parità,
   aggiungerlo in fase di migrazione.
3. Emettere nei log una riga con `open_cpu::runtime_summary()` consente di verificare a
   posteriori quale implementazione sia stata selezionata sulla macchina reale.

## 4.5 Esempio reale di migrazione (`open-raid-z`, 2026-08-22)

Come riferimento, ecco i punti salienti del primo caso reale di migrazione:

- Le `std::is_x86_feature_detected!` presenti in `detect_level()` sono state sostituite
  con riferimenti a `open_cpu::detect()` (l'enumerazione specifica del repository
  `SimdLevel` è stata lasciata intatta, spostando in open-cpu **solo gli elementi su cui
  si basa la decisione**). Così non è stato necessario modificare in alcun modo i
  chiamanti esistenti.
- Di `gf_mul_xor_into()` / `xor_into()` / `mul_pow2_xor_into()` è stato delegato a
  open-cpu **solo il percorso AVX2**, mentre per il percorso AVX-512 (non verificato
  anche sul lato open-cpu) e per il percorso SSE2 (non implementato in open-cpu) è stata
  mantenuta l'implementazione del repository. La scelta di fondo è: **non si effettuano
  sostituzioni che peggiorano le prestazioni**.
- I kernel SIMD non più utilizzati non sono stati eliminati ma lasciati in posto con
  `#[allow(dead_code)]`, come riferimento per una futura verifica incrociata con il lato
  open-cpu.

Si raccomanda questo modo di procedere: «non sostituire tutto in una volta, ma delegare
gradualmente a partire dalle parti equivalenti e senza perdita di prestazioni».

## 5. Avvertenze

- **Il percorso AVX-512 non è verificato in esecuzione** (la macchina di sviluppo non
  ne dispone). Poiché non viene selezionato per impostazione predefinita, la migrazione
  non altera il comportamento. Impostare `OPEN_CPU_ENABLE_AVX512=1` solo se si vuole
  verificare su una macchina con AVX-512.
- POPCNT/BMI/FMA/AES-NI/SHA-NI/VNNI sono **solo rilevati**. In `open-cpu` non esistono
  ancora implementazioni di checksum, compressione o operazioni matriciali che li usino,
  quindi il codice corrispondente sul lato `aruaru-db` / `aruaru-llm` non può ancora
  essere migrato (è però possibile migrare prima la sola parte di rilevamento).
- Su architetture diverse da x86 (ARM ecc.) tutte le funzionalità risultano `false` e si
  ricade sull'implementazione scalare. La cross-build riesce, ma le ottimizzazioni come
  NEON non sono supportate.
