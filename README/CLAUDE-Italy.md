> Originale giapponese / 日本語原文: [CLAUDE.md](../CLAUDE.md)

# Linee guida di sviluppo e regole dell'ambiente di sviluppo (open-cpu)

Questo repository fa parte dell'ecosistema `aon-co-jp`. **Per la filosofia di
progettazione e le regole operative comuni a tutti i repository (verifica
rigorosa, divieto di rapporti esagerati, evitare di reinventare la ruota,
prosecuzione automatica senza richiesta di conferma, ecc.) il testo di
riferimento è [`open-raid-z/CLAUDE.md`](https://github.com/aon-co-jp/open-raid-z/blob/main/CLAUDE.md).**
Qui non vengono duplicate: si riportano solo le questioni specifiche di questo
repository.

Il drive di lavoro è `F:\runo\open-cpu` (nuovo layout).

## Ruolo di questo repository

Poiché `open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` stavano per
implementare **ciascuno per conto proprio** il rilevamento a runtime e il dispatch
dei set di istruzioni CPU (AVX2/AVX-512/PCLMULQDQ/BMI1/BMI2/FMA3/AES-NI/POPCNT/SHA-NI),
questo repository è stato creato come base comune.

- **È un crate di libreria** (istruzione dell'utente, 2026-08-22). Non deve diventare
  un servizio residente (demone). Ogni repository lo aggiunge a `[dependencies]` e lo
  collega all'interno dello stesso processo. Non introdurre comunicazione tra processi.
- L'ambito si limita al «rilevamento delle funzionalità della CPU» e al «dispatch delle
  operazioni basato su tale rilevamento». Non trasferire qui la logica di dominio dei
  singoli repository (strategia di striping del RAID, gestione delle pagine del DB,
  caricamento dei modelli dell'LLM, ecc.).
- Non aumentare i crate di dipendenza (attualmente `[dependencies]` è vuoto, solo `std`).
  Essendo una base che si colloca allo strato più basso di ogni repository, introdurre
  dipendenze si ripercuote sull'intero insieme.

## Regole di implementazione e verifica (specifiche di questo repository)

- **Quando si aggiunge un'implementazione SIMD, scrivere contestualmente e
  obbligatoriamente un test di coincidenza dell'output con l'implementazione scalare.**
  Includere sempre nei casi di test i resti (lunghezze non multiple della larghezza
  del vettore).
- **I percorsi di codice che non è possibile eseguire su macchina reale vanno indicati
  esplicitamente come «non verificati» e non devono essere selezionati dal dispatch
  predefinito.** Attualmente rientra in questo caso il percorso AVX-512 (la macchina di
  sviluppo AMD Ryzen 9 3950X non dispone di AVX-512). Si abilita solo in opt-in tramite
  `OPEN_CPU_ENABLE_AVX512=1`. Quando la verifica su una macchina con AVX-512 sarà
  completata, rimuovere questa limitazione e aggiornare le diciture «non verificato» in
  README.md / PORTING.md / questo file.
- Quando si riportano valori di benchmark, indicare come «misurati» **solo quelli
  effettivamente misurati**. Non scrivere stime o valori teorici come se fossero misure.
- Non si usa `cargo bench`: è sufficiente la misurazione semplificata con
  `std::time::Instant` in `examples/bench.rs` (per non aumentare le dipendenze).

## Documentazione multilingue

Nella cartella `README/` sono collocate le versioni in 15 lingue di README / CLAUDE /
PORTING (prassi comune dell'ecosistema, stessa convenzione di denominazione di
`open-raid-z` / `open-cuda`). **La versione giapponese (`README.md` / `CLAUDE.md` /
`PORTING.md` nella radice del repository) è il testo di riferimento**: quando se ne
aggiorna il contenuto, occorre allineare anche le versioni nelle 15 lingue (non esiste
un meccanismo di sincronizzazione automatica, va fatto manualmente).

Lingue: US English / UK English / Germany / Italy / France / Spain / Russia /
Ukraine / Hebrew / Persian (Iran) / Arabic / China / Taiwan / Korea / Japan.

## HANDOFF

- **2026-08-22 nuova creazione + integrazione in 2 repository**:
  a partire da un repository vuoto è stata realizzata l'implementazione iniziale.
  - `src/caps.rs`: struttura `CpuCapabilities` e `detect()` (cache `OnceLock`).
    Gli elementi rilevati sono avx2 / avx512f / avx512bw / avx512vl / pclmulqdq / bmi1 / bmi2 /
    fma / aes / popcnt / sha / sse2 / ssse3 / avx-vnni / avx512vnni.
  - `src/gf.rs`: operazioni GF(2^8) di RAID6 (polinomio 0x11d, generatore g=2).
    Espone `gf_xor` / `gf_mul_parity` / `raid6_parity` / `raid6_coeff` e effettua il
    dispatch a runtime tra le 4 implementazioni scalare / PCLMULQDQ / AVX2 / AVX-512.
  - Misure (Ryzen 9 3950X, 4MiB×50 volte): scalar 1003 MiB/s, pclmulqdq 1916 MiB/s
    (1.91x), **avx2 22531 MiB/s (22.46x)**. AVX-512 non misurabile perché assente.
    → **In seguito si è scoperto che questo fattore avx2, in nuove misurazioni, oscilla
    nell'intervallo 11,6–18,1 volte** (perché il lato SIMD è limitato dalla banda di
    memoria. I valori e gli intervalli più aggiornati sono quelli della sezione
    «Benchmark misurati» di README.md).
  - `cargo test --release`: tutti gli 11 test + 1 doctest superati. È stato verificato
    che l'implementazione PCLMULQDQ coincide con quella scalare per tutti i 256 coefficienti.
  - Integrazioni realizzate: incorporato tramite dipendenza path in `open-raid-z`
    (sostituzione delle operazioni GF) e in `open-english/server`
    (endpoint `/v1/cpu-runtime` e log di avvio).

- **2026-08-22 (seguito) ciclo di miglioramento della praticità + funzione di
  autopresentazione**:
  dopo l'implementazione iniziale è stato svolto un ciclo sviluppo→TEST→correzione per
  aumentare interoperabilità e praticità.
  - **Ciclo 1: aggiunta delle API del metodo di Horner** (`gf_mul_pow2_xor` /
    `gf_mul2_xor` / `gf_mul4_xor`, AVX2 + scalare). Ciò ha permesso di delegare a
    open-cpu anche il percorso AVX2 di `mul2_xor_into` / `mul4_xor_into` di
    `open-raid-z`, riducendo ulteriormente il codice x86 di quel lato.
    È stato aggiunto un percorso AVX2 anche a `gf_xor` (affinché la delega non causasse
    una regressione prestazionale).
  - **Ciclo 2: miglioramento dell'usabilità.** Aggiunti a `CpuCapabilities`
    l'implementazione di `Display` e `has_all()`. Aggiunta `raid6_parity3()` che calcola
    in blocco P/Q/R equivalenti a RAID-Z3. Aggiunte a `examples/bench.rs` le misurazioni
    di XOR e metodo di Horner.
  - **Verifica**: `cargo test --release` **tutti i 15 test + 2 doctest superati**.
    Il benchmark è stato eseguito 4 volte di seguito e la variabilità (oscillazione tra
    11,6 e 18,1 volte perché il lato SIMD è limitato dalla banda di memoria) è stata
    registrata onestamente nel README sotto forma di intervallo.
  - **Un bug reale trovato integrando in `open-english` (lezione importante)**: la
    funzione di risposta di autopresentazione alla domanda «chi lo ha creato» era stata
    implementata con una corrispondenza parziale di parola chiave (`"誰が作"`), e
    **nel test su browser reale è emerso che «誰が【このシステムを】作ったのですか?»
    non veniva rilevata** (perché tra l'interrogativo e il verbo si inseriscono altre
    parole). È stata corretta separando la lista degli interrogativi da quella dei verbi
    e valutandole in AND, con verifica di 10 esempi positivi e 6 negativi e conferma E2E
    su browser reale. Resta come annotazione: **era un tipo di bug che non si trova
    fermandosi a «i test unitari passano», ma solo arrivando a «provare davvero a
    digitare nella chat»**.

- **Cose da fare in seguito**:
  1. Integrare progressivamente anche nella parte di checksum e compressione di
     `aruaru-db`, nelle operazioni matriciali di `aruaru-llm` e nel fallback su CPU di
     `open-cuda` (attualmente sono solo i 2 casi `open-raid-z` e `open-english`).
  2. Se sarà possibile procurarsi o riservare una macchina con AVX-512, verificare con
     misure reali il percorso AVX-512 e rimuovere la limitazione all'opt-in.
  3. Per i set di istruzioni solo rilevati e privi di implementazione
     (POPCNT/BMI/FMA/AES-NI/SHA-NI), aggiungere implementazioni di calcolo a partire da
     quelli che diventeranno effettivamente necessari sul lato dipendente. In
     particolare, per i checksum di `aruaru-db` è candidato il CRC32C basato su
     PCLMULQDQ e per `aruaru-llm` il prodotto scalare int8 con AVX-VNNI/AVX-512 VNNI.
  4. Poiché è configurato con dipendenza path (`path = "../open-cpu"`), per compilare in
     ambienti privi del layout `F:\runo` (come il VPS) sarà necessario passare a una
     dipendenza git. Si affronterà quando si arriverà effettivamente a compilare sul VPS.
  5. Non esistendo un'implementazione per CPU con solo SSE2, `open-raid-z` mantiene una
     propria implementazione unicamente per il percorso SSE2. Aggiungendo una versione
     SSE2 si potrebbe delegare anche quella.
  6. `gf_xor`, anche vettorizzato, rende solo 1,12–1,25 volte rispetto allo scalare
     (limite della banda di memoria). È possibile che risulti efficace un'ottimizzazione
     con store non temporali (`_mm256_stream_si256`) per evitare l'inquinamento della
     cache: vale la pena provarla per buffer di grandi dimensioni (non verificata).
