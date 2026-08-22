> Original japonais / 日本語原文: [CLAUDE.md](../CLAUDE.md)

# Politique de développement & règles d'environnement de développement (open-cpu)

Ce dépôt fait partie de l'écosystème `aon-co-jp`. **Pour la philosophie de
conception et les règles d'exploitation communes à tous les dépôts (vérification
rigoureuse, interdiction des rapports exagérés, éviter de réinventer la roue,
poursuite automatique sans demande de confirmation, etc.), le texte de référence
est [`open-raid-z/CLAUDE.md`](https://github.com/aon-co-jp/open-raid-z/blob/main/CLAUDE.md).**
Il n'est pas recopié ici : seuls les points propres à ce dépôt y figurent.

Le lecteur de travail est `F:\runo\open-cpu` (nouvelle disposition).

## Rôle de ce dépôt

Comme `open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` s'apprêtaient
**chacun de leur côté** à implémenter la détection à l'exécution et le dispatch
des jeux d'instructions CPU
(AVX2/AVX-512/PCLMULQDQ/BMI1/BMI2/FMA3/AES-NI/POPCNT/SHA-NI), ce dépôt a été
créé comme socle commun.

- **C'est une crate de bibliothèque** (instruction de l'utilisateur,
  2026-08-22). Elle ne doit pas devenir un service résident (démon). Chaque
  dépôt l'ajoute dans ses `[dependencies]` et la lie à l'intérieur du même
  processus. Ne pas introduire de communication interprocessus.
- La portée se limite à « la détection des fonctionnalités CPU » et « le
  dispatch des calculs fondé sur le résultat de cette détection ». Ne pas
  transférer ici la logique métier de chaque dépôt (stratégie de striping du
  RAID, gestion des pages de la base de données, chargement des modèles du LLM,
  etc.).
- Ne pas multiplier les crates dépendantes (actuellement `[dependencies]` est
  vide, seule `std` est utilisée). Comme il s'agit d'un socle placé tout en bas
  de la pile de chaque dépôt, y introduire une dépendance se répercuterait sur
  l'ensemble.

## Règles d'implémentation et de vérification (propres à ce dépôt)

- **Dès qu'une implémentation SIMD est ajoutée, écrire en même temps un test de
  concordance des sorties avec l'implémentation scalaire.** Inclure
  impérativement dans les cas de test les restes (longueurs qui ne sont pas des
  multiples de la largeur de vecteur).
- **Tout chemin de code qui ne peut pas être exécuté sur la machine réelle doit
  être explicitement noté « non vérifié » et ne doit pas être choisi par le
  dispatch par défaut.** C'est actuellement le cas du chemin AVX-512 (l'AMD
  Ryzen 9 3950X de la machine de développement n'a pas AVX-512). Il ne s'active
  qu'en opt-in via `OPEN_CPU_ENABLE_AVX512=1`. Une fois la vérification faite
  sur une machine dotée d'AVX-512, lever cette restriction et mettre à jour la
  mention « non vérifié » dans README.md / PORTING.md et dans ce fichier.
- Lorsqu'on inscrit des chiffres de benchmark, ne qualifier de « mesuré » que
  **ce qui a réellement été mesuré**. Ne pas présenter des valeurs estimées ou
  théoriques comme des mesures.
- Ne pas utiliser `cargo bench` : la mesure simplifiée par
  `std::time::Instant` dans `examples/bench.rs` suffit (afin de ne pas
  multiplier les dépendances).

## Documentation multilingue

Le dossier `README/` contient les versions en 15 langues de README / CLAUDE /
PORTING (pratique commune à l'écosystème, mêmes conventions de nommage que
`open-raid-z` / `open-cuda`). **Les versions japonaises (`README.md` /
`CLAUDE.md` / `PORTING.md` à la racine du dépôt) font foi** ; lorsqu'on en
modifie le contenu, les 15 versions traduites doivent suivre (il n'existe aucun
mécanisme de synchronisation automatique, la répercussion est manuelle).

Langues : US English / UK English / Germany / Italy / France / Spain / Russia /
Ukraine / Hebrew / Persian (Iran) / Arabic / China / Taiwan / Korea / Japan.

## HANDOFF

- **2026-08-22 création + intégration dans 2 dépôts** :
  l'implémentation initiale a été créée à partir d'un dépôt vide.
  - `src/caps.rs` : la structure `CpuCapabilities` et `detect()` (cache
    `OnceLock`). Éléments détectés : avx2 / avx512f / avx512bw / avx512vl /
    pclmulqdq / bmi1 / bmi2 / fma / aes / popcnt / sha / sse2 / ssse3 /
    avx-vnni / avx512vnni.
  - `src/gf.rs` : opérations GF(2^8) de RAID6 (polynôme 0x11d, générateur g=2).
    `gf_xor` / `gf_mul_parity` / `raid6_parity` / `raid6_coeff` sont publics et
    les 4 implémentations scalaire / PCLMULQDQ / AVX2 / AVX-512 sont
    sélectionnées par dispatch à l'exécution.
  - Mesures (Ryzen 9 3950X, 4MiB×50 fois) : scalar 1003 MiB/s, pclmulqdq
    1916 MiB/s (1.91x), **avx2 22531 MiB/s (22.46x)**. AVX-512 n'étant pas
    présent, aucune mesure n'est possible.
    → **Il s'est avéré par la suite, lors d'une nouvelle mesure, que ce facteur
    avx2 varie dans une plage de 11,6 à 18,1×** (le côté SIMD étant limité par
    la bande passante mémoire ; les chiffres et plages à jour figurent dans la
    section « Benchmarks mesurés » du README.md, qui fait foi).
  - `cargo test --release` : les 11 tests et 1 doctest passent tous.
    L'implémentation PCLMULQDQ a été vérifiée comme concordant avec la version
    scalaire pour l'ensemble des 256 coefficients.
  - Intégrations réalisées : intégrée par dépendance path dans `open-raid-z`
    (remplacement des opérations GF) et `open-english/server` (point d'entrée
    `/v1/cpu-runtime` et journal de démarrage).

- **2026-08-22 (suite) cycle d'amélioration de l'utilité + fonction de
  présentation de soi** :
  après l'implémentation initiale, un cycle développement → TEST → correction a
  été mené pour améliorer l'interopérabilité et l'utilité pratique.
  - **Cycle 1 : ajout des API de la méthode de Horner** (`gf_mul_pow2_xor` /
    `gf_mul2_xor` / `gf_mul4_xor`, AVX2 + scalaire). Cela a permis de déléguer
    aussi à open-cpu le chemin AVX2 de `mul2_xor_into` / `mul4_xor_into` de
    `open-raid-z`, et donc de réduire encore le code x86 de ce côté-là. Un
    chemin AVX2 a également été ajouté à `gf_xor` (afin que la délégation ne
    provoque pas de régression de performance).
  - **Cycle 2 : amélioration de l'ergonomie**. Ajout d'une implémentation de
    `Display` et de `has_all()` à `CpuCapabilities`. Ajout de
    `raid6_parity3()`, qui calcule en une fois les P/Q/R équivalents à
    RAID-Z3. Ajout dans `examples/bench.rs` de la mesure du XOR et de la
    méthode de Horner.
  - **Vérification** : `cargo test --release` — **les 15 tests et les 2
    doctests passent tous**. Le benchmark a été exécuté 4 fois de suite et la
    dispersion (variation de 11,6 à 18,1× parce que le côté SIMD est limité par
    la bande passante mémoire) a été honnêtement consignée sous forme de plage
    dans le README.
  - **Un vrai bug découvert lors de l'intégration à `open-english` (leçon
    importante)** : la fonction de réponse de présentation de soi à la question
    « qui l'a fabriqué ? » avait été implémentée par correspondance partielle de
    mots-clés (`"誰が作"`), et **un test dans un vrai navigateur a révélé que
    « 誰が【このシステムを】作ったのですか? » n'était pas détecté** (parce que
    des mots s'intercalent entre l'interrogatif et le verbe). Cela a été corrigé
    en séparant la liste des interrogatifs et celle des verbes et en les
    combinant par un ET logique, puis vérifié sur 10 exemples positifs et
    6 exemples négatifs ainsi que par un contrôle E2E dans un vrai navigateur.
    C'est le relevé d'**un bug qu'on ne trouve pas en se contentant de « les
    tests unitaires passent », mais seulement en allant jusqu'à réellement
    saisir la question dans le chat**.

- **À faire ensuite** :
  1. Intégrer progressivement aussi aux sommes de contrôle et à la compression
     d'`aruaru-db`, au calcul matriciel d'`aruaru-llm` et au repli CPU
     d'`open-cuda` (pour l'instant, seuls `open-raid-z` et `open-english`, soit
     2 cas).
  2. Dès qu'une machine dotée d'AVX-512 pourra être obtenue ou réservée,
     vérifier le chemin AVX-512 par des mesures réelles et lever la restriction
     d'opt-in.
  3. Pour les jeux d'instructions seulement détectés et sans implémentation
     (POPCNT/BMI/FMA/AES-NI/SHA-NI), ajouter des implémentations de calcul en
     commençant par ceux dont un dépôt dépendant a réellement besoin. En
     particulier, CRC32C par PCLMULQDQ pour les sommes de contrôle d'`aruaru-db`
     et le produit scalaire int8 par AVX-VNNI/AVX-512 VNNI pour `aruaru-llm`
     sont des candidats.
  4. Comme le montage se fait par dépendance path (`path = "../open-cpu"`), il
     faudra basculer sur une dépendance git pour compiler dans un environnement
     dépourvu de la disposition `F:\runo`, tel qu'un VPS. On s'en occupera au
     moment où l'on compilera effectivement sur le VPS.
  5. Faute d'implémentation pour les CPU disposant uniquement de SSE2,
     `open-raid-z` conserve sa propre implémentation pour le seul chemin SSE2.
     L'ajout d'une version SSE2 permettrait de déléguer cela aussi.
  6. Même vectorisé, `gf_xor` ne dépasse pas 1,12 à 1,25× la version scalaire
     (limité par la bande passante mémoire). Une optimisation par stockage non
     temporel (`_mm256_stream_si256`) évitant la pollution du cache pourrait
     être efficace et mérite d'être essayée pour les gros tampons (non vérifié).
