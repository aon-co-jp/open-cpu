> Originale giapponese / 日本語原文: [README.md](../README.md)

# open-cpu

**Libreria di rilevamento dei set di istruzioni CPU e di dispatch a runtime**
(Rust), comune all'ecosistema `aon-co-jp`.

È stata creata per evitare che `open-raid-z` / `aruaru-db` / `aruaru-llm` /
`open-cuda` scrivano ciascuno il proprio codice duplicato di rilevamento delle
funzionalità della CPU.

**Non è un servizio residente (demone).** È un normale crate di libreria che
ogni repository aggiunge alla sezione `[dependencies]` del proprio
`Cargo.toml` e collega all'interno dello stesso processo.

## Cosa può fare

1. **Rilevamento a runtime delle funzionalità della CPU** — `open_cpu::detect()`
   restituisce `&'static CpuCapabilities`. Usa `std::is_x86_feature_detected!` e
   memorizza in cache il risultato del primo rilevamento in un `OnceLock`, quindi
   il costo delle chiamate successive è quasi nullo.
2. **Dispatch a runtime delle operazioni RAID6 su GF(2^8)** — in base al risultato
   del rilevamento seleziona a runtime l'implementazione scalare / PCLMULQDQ /
   AVX2 / AVX-512.

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

### Elenco delle API pubbliche

| API | Contenuto | Dispatch |
|---|---|---|
| `detect() -> &'static CpuCapabilities` | Rilevamento delle funzionalità della CPU (cache `OnceLock`) | — |
| `runtime_summary() -> String` | Riepilogo su una riga del rilevamento e dell'implementazione scelta | — |
| `selected_impl() -> GfImpl` | Implementazione selezionata per le operazioni GF | — |
| `gf_xor(dst, src)` | `dst ^= src` (parità P) | AVX2 / scalare |
| `gf_mul_parity(dst, src, factor)` | `dst ^= src * factor` (parità Q) | AVX-512 (opt-in) / AVX2 / PCLMULQDQ / scalare |
| `gf_mul_pow2_xor(acc, src, times)` | `acc = acc * 2^times ^ src` | AVX2 / scalare |
| `gf_mul2_xor` / `gf_mul4_xor` | Le precedenti con `times=1` / `times=2` | Come sopra |
| `raid6_parity(stripes, p, q)` | Calcolo cumulativo di P/Q con il metodo della tabella dei coefficienti | Come sopra |
| `raid6_parity3(stripes, p, q, r)` | Calcolo cumulativo di P/Q/R con il metodo di Horner | Come sopra |
| `gf_mul(a, b) -> u8` | Moltiplicazione GF su un byte (`const fn`) | — |
| `gf_mul2_byte(b) -> u8` | Raddoppio in GF di un byte (`const fn`) | — |
| `raid6_coeff(i) -> u8` | Coefficiente RAID6 `g^i` (`g = 2`) | — |

Sono pubbliche anche le versioni `*_scalar` / `*_avx2` / `*_pclmul` / `*_avx512`
che invocano esplicitamente ciascuna implementazione (per benchmark e verifica
incrociata; le versioni SIMD sono `unsafe`).

## Set di istruzioni rilevati

| Set di istruzioni | Rilevamento | Utilizzo in questo crate |
|---|---|---|
| SSE2 | ✅ | Usato come supporto del percorso PCLMULQDQ |
| SSSE3 | ✅ | `pshufb` (riduzione del percorso PCLMULQDQ) |
| PCLMULQDQ | ✅ | Moltiplicazione GF(2^8) (implementazione con moltiplicazione carry-less) |
| AVX2 | ✅ | Moltiplicazione GF(2^8) (split-table con `vpshufb`, selezionata per impostazione predefinita) |
| AVX-512F / BW / VL | ✅ | Esiste un percorso di moltiplicazione GF(2^8) (**esecuzione non verificata**, vedi sotto) |
| POPCNT | ✅ | Solo rilevamento (nessuna implementazione che lo usi) |
| BMI1 / BMI2 | ✅ | Solo rilevamento (nessuna implementazione che lo usi) |
| FMA3 | ✅ | Solo rilevamento (nessuna implementazione che lo usi) |
| AES-NI | ✅ | Solo rilevamento (nessuna implementazione che lo usi) |
| SHA-NI | ✅ | Solo rilevamento (nessuna implementazione che lo usi) |
| AVX-VNNI | ✅ | Solo rilevamento (in vista di future inferenze AI, nessuna implementazione che lo usi) |
| AVX-512 VNNI | ✅ | Solo rilevamento (in vista di future inferenze AI, nessuna implementazione che lo usi) |

Su architetture diverse da x86/x86_64 tutti i campi valgono `false` e si ricade
sull'implementazione scalare (la compilazione riesce comunque).

## Dettagli dell'implementazione GF(2^8)

Il polinomio irriducibile è `0x11d` (x^8+x^4+x^3+x^2+1), il generatore è `g = 2`.
Gli stessi di Linux md/RAID6 e di ZFS RAID-Z.

- **Scalare**: implementazione di riferimento con tabella nibble split (16 voci × 2).
- **PCLMULQDQ**: espandendo ogni byte a intervalli di 16 bit, il prodotto carry-less
  con un coefficiente a 8 bit (al massimo 15 bit) non riporta nello slot adiacente.
  Sfruttando questa proprietà si moltiplicano 4 byte insieme con una sola istruzione
  e si riduce a GF(2^8) con due `pshufb`. 16 byte/iter.
- **AVX2**: implementazione split-table con `vpshufb`. 32 byte/iter.
- **AVX-512F/BW**: stessa split-table elaborata a 64 byte/iter.

## Benchmark misurati

`cargo run --release --example bench` (4 MiB × 50 volte = 200 MiB, factor=0x8d)

Macchina di sviluppo: **AMD Ryzen 9 3950X** / Windows 11 / rustc 1.96.0

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

**Sulla variabilità dei valori misurati (annotazione onesta)**: eseguendo 4 volte
di seguito, il fattore AVX2 della moltiplicazione GF ha oscillato tra
**11,6 e 18,1 volte**, il metodo di Horner tra **2,70 e 3,52 volte** e lo XOR tra
**1,12 e 1,25 volte** (il lato scalare è rimasto stabile attorno ai 207 ms).
Poiché il lato SIMD raggiunge da 10.000 a 20.000 MiB/s ed è quindi **limitato dalla
banda di memoria**, risente facilmente dello stato della cache e di altri processi.
La tabella qui sopra riporta tal quale il risultato di una singola esecuzione: è più
corretto interpretare i fattori come intervalli.

- **La moltiplicazione GF(2^8) per un coefficiente arbitrario è 11,6–18,1 volte più
  veloce con AVX2 rispetto allo scalare (misurato)**. Ha effetto sul percorso della
  parità Q e del ripristino in RAID6.
- **Il metodo di Horner è 2,70–3,52 volte più veloce (misurato)**. Poiché la versione
  scalare è già ottimizzata con trucchi sui bit a u64, la differenza non è grande
  quanto per la moltiplicazione GF.
- **Il semplice XOR è 1,12–1,25 volte più veloce**. Già la versione scalare (u64) è
  incollata alla banda di memoria, quindi il margine per la vettorizzazione è ridotto
  (come previsto).
- PCLMULQDQ è 1,90 volte più veloce e ha senso solo come **fallback per le CPU
  vecchie in cui AVX2 non è disponibile**.

## Stato della verifica (divulgazione onesta)

- ✅ **Scalare / PCLMULQDQ / AVX2**: verificati in esecuzione sulla macchina di
  sviluppo sopra indicata. Con `cargo test` si verifica la correttezza
  dell'implementazione scalare prendendo come riferimento un'implementazione
  ingenua a scorrimento di bit, e poi, prendendo come riferimento l'implementazione
  scalare, la coincidenza dell'output dell'implementazione PCLMULQDQ (**tutti i 256
  coefficienti × 9 lunghezze**) e dell'implementazione AVX2 (8 coefficienti × 11
  lunghezze, compresa la gestione dei resti). Tutti i 15 test + 2 doctest superati.
- ⚠️ **Il percorso AVX-512 non è verificato in esecuzione.** Poiché la macchina di
  sviluppo (Ryzen 9 3950X) non dispone di AVX-512, si è verificato **solo che la
  compilazione riesca**. Per sicurezza non viene selezionato dal dispatch
  predefinito e si abilita in opt-in solo impostando la variabile d'ambiente
  `OPEN_CPU_ENABLE_AVX512=1`. Questo trattamento resterà in vigore finché non sarà
  completata la verifica su una macchina con AVX-512.
- ⚠️ Per POPCNT/BMI1/BMI2/FMA/AES-NI/SHA-NI/VNNI **esistono solo i campi di
  rilevamento**: non c'è ancora alcuna implementazione di calcolo che li usi.

## Esecuzione di test e benchmark

```
cargo build --release
cargo test --release
cargo run --release --example bench
```

## Adozioni (al 2026-08-22)

| Repository | Uso |
|---|---|
| [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z) | Delega a questo crate del rilevamento delle funzionalità della CPU, della moltiplicazione GF(2^8), dello XOR e del metodo di Horner (percorso AVX2) in `zfs_accel_hlsl/src/simd.rs`. Anche dopo la migrazione i 39 test esistenti passano tutti e i valori numerici coincidono perfettamente. |
| [`open-english`](https://github.com/aon-co-jp/open-english) | Riepilogo su una riga nel log di avvio del server e `GET /v1/cpu-runtime` (restituisce in JSON i set di istruzioni CPU della piattaforma di esecuzione). |

Non ancora adottato (obiettivi futuri): `aruaru-db` (checksum e compressione),
`aruaru-llm` (operazioni matriciali), `open-cuda` (fallback su CPU in assenza di GPU).

## Collegamenti

- Procedura di migrazione: [PORTING.md](PORTING-Italy.md)
- Linee guida di sviluppo e HANDOFF: [CLAUDE.md](CLAUDE-Italy.md)
- GitHub organization: https://github.com/aon-co-jp
