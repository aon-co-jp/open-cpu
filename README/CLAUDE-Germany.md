> Japanisches Original / 日本語原文: [CLAUDE.md](../CLAUDE.md)

# Entwicklungsrichtlinien & Regeln für die Entwicklungsumgebung (open-cpu)

Diese Repository ist Teil des `aon-co-jp`-Ökosystems. **Für die allen Repositories
gemeinsamen Designgrundsätze und Betriebsregeln (gründliche Verifizierung, Verbot
übertriebener Berichte, Vermeidung des Neuerfindens des Rades, automatisches
Fortsetzen ohne Rückfrage usw.) gilt
[`open-raid-z/CLAUDE.md`](https://github.com/aon-co-jp/open-raid-z/blob/main/CLAUDE.md)
als maßgebliche Fassung.** Sie werden hier nicht dupliziert; hier stehen nur die für
diese Repository spezifischen Punkte.

Das Arbeitslaufwerk ist `F:\runo\open-cpu` (neues Layout).

## Rolle dieser Repository

Da `open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` **jeweils eigenständig**
eine Laufzeiterkennung und einen Dispatch für CPU-Befehlssätze
(AVX2/AVX-512/PCLMULQDQ/BMI1/BMI2/FMA3/AES-NI/POPCNT/SHA-NI) implementieren wollten,
wurde diese Repository als gemeinsame Basis dafür neu angelegt.

- **Es handelt sich um eine Bibliotheks-Crate** (Anweisung des Nutzers, 2026-08-22).
  Sie wird nicht zu einem residenten Dienst (Daemon) gemacht. Jede Repository trägt
  sie unter `[dependencies]` ein und linkt sie innerhalb desselben Prozesses.
  Interprozesskommunikation ist nicht einzuführen.
- Der Umfang beschränkt sich auf „CPU-Feature-Erkennung“ und „Dispatch von Operationen
  auf Basis dieser Erkennungsergebnisse“. Die Domänenlogik der einzelnen Repositories
  (Striping-Strategie von RAID, Seitenverwaltung der DB, Laden von LLM-Modellen usw.)
  darf nicht hierher portiert werden.
- Keine zusätzlichen abhängigen Crates (derzeit ist `[dependencies]` leer, nur `std`).
  Da dies die unterste Schicht jeder Repository bildet, würden eingebrachte
  Abhängigkeiten sich auf das Ganze auswirken.

## Implementierungs- und Verifizierungsregeln (spezifisch für diese Repository)

- **Wird eine SIMD-Implementierung hinzugefügt, ist zwingend gleichzeitig ein Test auf
  Übereinstimmung der Ausgabe mit der Skalarimplementierung zu schreiben.**
  Restfälle (Längen, die kein Vielfaches der Vektorbreite sind) müssen zwingend in den
  Testfällen enthalten sein.
- **Codepfade, die auf echter Hardware nicht ausgeführt werden können, sind ausdrücklich
  als „nicht verifiziert“ zu kennzeichnen und dürfen im Standard-Dispatch nicht gewählt
  werden.** Derzeit trifft dies auf den AVX-512-Pfad zu (der Entwicklungsrechner
  AMD Ryzen 9 3950X besitzt kein AVX-512). Er ist nur per opt-in über
  `OPEN_CPU_ENABLE_AVX512=1` aktiv. Sobald die Verifizierung auf einem Rechner mit
  AVX-512 abgeschlossen ist, ist diese Einschränkung aufzuheben und die Kennzeichnung
  „nicht verifiziert“ in README.md / PORTING.md / dieser Datei zu aktualisieren.
- Beim Notieren von Benchmark-Zahlen darf **nur tatsächlich Gemessenes** als „gemessen“
  bezeichnet werden. Schätz- oder Theoriewerte dürfen nicht wie Messwerte dargestellt
  werden.
- `cargo bench` wird nicht verwendet; die einfache Messung über `std::time::Instant`
  in `examples/bench.rs` genügt (um keine Abhängigkeiten hinzuzufügen).

## Mehrsprachige Dokumentation

Im Ordner `README/` liegen README / CLAUDE / PORTING in 15 Sprachfassungen
(gemeinsame Praxis des Ökosystems, gleiche Namenskonvention wie bei
`open-raid-z` / `open-cuda`). **Maßgeblich ist die japanische Fassung
(`README.md` / `CLAUDE.md` / `PORTING.md` direkt im Wurzelverzeichnis der
Repository)**; werden deren Inhalte aktualisiert, sind die 15 Sprachfassungen
nachzuziehen (es gibt keinen Mechanismus zur automatischen Synchronisation,
die Übernahme erfolgt manuell).

Sprachen: US English / UK English / Germany / Italy / France / Spain / Russia /
Ukraine / Hebrew / Persian (Iran) / Arabic / China / Taiwan / Korea / Japan.

## HANDOFF

- **2026-08-22 Neuanlage + Integration in 2 Repositories**:
  Ausgehend von einer leeren Repository wurde die Erstimplementierung erstellt.
  - `src/caps.rs`: Struktur `CpuCapabilities` und `detect()` (`OnceLock`-Cache).
    Erkannt werden avx2 / avx512f / avx512bw / avx512vl / pclmulqdq / bmi1 / bmi2 /
    fma / aes / popcnt / sha / sse2 / ssse3 / avx-vnni / avx512vnni.
  - `src/gf.rs`: GF(2^8)-Operationen für RAID6 (Polynom 0x11d, erzeugendes Element g=2).
    `gf_xor` / `gf_mul_parity` / `raid6_parity` / `raid6_coeff` sind öffentlich, und die
    4 Implementierungen Skalar / PCLMULQDQ / AVX2 / AVX-512 werden zur Laufzeit
    dispatcht.
  - Messwerte (Ryzen 9 3950X, 4MiB×50 Durchläufe): scalar 1003 MiB/s, pclmulqdq
    1916 MiB/s (1.91x), **avx2 22531 MiB/s (22.46x)**. AVX-512 ist nicht vorhanden,
    daher nicht messbar.
    → **Es zeigte sich, dass dieser AVX2-Faktor bei einer späteren Neumessung im
    Bereich von 11,6–18,1 schwankt** (da die SIMD-Seite durch die Speicherbandbreite
    begrenzt ist. Maßgeblich für die aktuellen Zahlen und Bereiche ist der Abschnitt
    „Gemessene Benchmarks“ in README.md).
  - `cargo test --release`: alle 11 Tests + 1 doctest bestanden. Für die
    PCLMULQDQ-Implementierung wurde die Übereinstimmung mit der Skalarvariante über
    alle 256 Koeffizienten bestätigt.
  - Integrationsstand: In `open-raid-z` (Ersetzung der GF-Operationen) und
    `open-english/server` (Endpunkt `/v1/cpu-runtime` und Startlog) bereits per
    path-Abhängigkeit eingebunden.

- **2026-08-22 (Fortsetzung) Zyklus zur Steigerung der Praxistauglichkeit +
  Selbstvorstellungsfunktion**:
  Nach der Erstimplementierung wurde ein Zyklus aus Entwicklung → TEST → Korrektur
  durchgeführt, um Integrierbarkeit und Praxistauglichkeit zu erhöhen.
  - **Zyklus 1: Ergänzung der Horner-Schema-API** (`gf_mul_pow2_xor` /
    `gf_mul2_xor` / `gf_mul4_xor`, AVX2 + Skalar). Dadurch konnten auch die
    AVX2-Pfade von `mul2_xor_into` / `mul4_xor_into` in `open-raid-z` an
    open-cpu delegiert und der dortige x86-Code weiter reduziert werden.
    Auch `gf_xor` erhielt einen AVX2-Pfad (damit die Delegation keine
    Performance-Regression bedeutet).
  - **Zyklus 2: Verbesserung der Handhabbarkeit**. `CpuCapabilities` wurde um eine
    `Display`-Implementierung und `has_all()` ergänzt. Hinzugefügt wurde
    `raid6_parity3()`, das P/Q/R entsprechend RAID-Z3 gesammelt berechnet. In
    `examples/bench.rs` wurden Messungen für XOR und das Horner-Schema ergänzt.
  - **Verifizierung**: `cargo test --release` — **alle 15 Tests + 2 doctests
    bestanden**. Der Benchmark wurde viermal hintereinander ausgeführt, und die
    Streuung (Schwankung von 11,6–18,1×, da die SIMD-Seite durch die
    Speicherbandbreite begrenzt ist) wurde im README ehrlich als Bereich festgehalten.
  - **Ein bei der Integration in `open-english` gefundener echter Bug (wichtige
    Lehre)**: Die Selbstvorstellungsantwort auf die Frage „Wer hat dich gemacht?“
    war über eine Teilstring-Übereinstimmung eines Schlüsselworts
    (`"誰が作"`) implementiert; **beim Test im echten Browser zeigte sich, dass
    „誰が【このシステムを】作ったのですか?“ nicht erkannt wurde** (weil zwischen
    Fragewort und Verb weitere Wörter stehen). Es wurde auf eine Prüfung umgestellt,
    die Fragewortliste und Verbliste trennt und mit UND-Bedingung auswertet; dazu
    wurden 10 positive und 6 negative Beispiele geprüft sowie eine E2E-Prüfung im
    echten Browser durchgeführt. Festgehalten sei: Es war **ein Bug jener Art, den man
    nicht findet, wenn man es bei „die Unittests bestehen“ belässt und nicht so weit
    geht, tatsächlich etwas in den Chat einzugeben.**

- **Nächste Schritte**:
  1. Nach und nach auch in den Prüfsummen-/Kompressionsbereich von `aruaru-db`, die
     Matrixoperationen von `aruaru-llm` und den CPU-Fallback von `open-cuda`
     integrieren (derzeit nur die 2 Fälle `open-raid-z` und `open-english`).
  2. Sobald ein Rechner mit AVX-512 beschafft bzw. verfügbar ist, den AVX-512-Pfad
     messtechnisch verifizieren und die opt-in-Beschränkung aufheben.
  3. Für Befehlssätze, die nur erkannt, aber nicht implementiert sind
     (POPCNT/BMI/FMA/AES-NI/SHA-NI), Rechenimplementierungen in der Reihenfolge
     ergänzen, in der sie auf der abhängigen Seite tatsächlich benötigt werden.
     Kandidaten sind insbesondere CRC32C mittels PCLMULQDQ für die Prüfsummen von
     `aruaru-db` sowie int8-Skalarprodukte mittels AVX-VNNI/AVX-512 VNNI für
     `aruaru-llm`.
  4. Da mit path-Abhängigkeit (`path = "../open-cpu"`) gearbeitet wird, ist beim Bauen
     in Umgebungen ohne das `F:\runo`-Layout (z. B. VPS) ein Wechsel zur
     git-Abhängigkeit nötig. Das wird angegangen, sobald tatsächlich auf dem VPS
     gebaut wird.
  5. Da es keine Implementierung für CPUs mit nur SSE2 gibt, behält `open-raid-z`
     allein für den SSE2-Pfad eine eigene Implementierung. Mit einer SSE2-Version
     ließe sich auch dieser Teil delegieren.
  6. `gf_xor` erreicht selbst mit SIMD nur den Faktor 1,12–1,25 gegenüber Skalar
     (Begrenzung durch die Speicherbandbreite). Eine Optimierung mit nicht-temporalen
     Stores (`_mm256_stream_si256`), die Cache-Verschmutzung vermeidet, könnte wirksam
     sein und wäre für große Puffer einen Versuch wert (nicht verifiziert).
