> Original japonais / 日本語原文: [README.md](../README.md)

# open-cpu

**Bibliothèque de détection des jeux d'instructions CPU et de dispatch à
l'exécution** (Rust), commune à l'écosystème `aon-co-jp`.

Elle a été créée pour éviter que `open-raid-z` / `aruaru-db` / `aruaru-llm` /
`open-cuda` n'écrivent chacun leur propre code de détection des fonctionnalités
CPU en double.

**Ce n'est pas un service résident (démon).** C'est une simple crate de
bibliothèque que chaque dépôt ajoute dans la section `[dependencies]` de son
`Cargo.toml` et lie à l'intérieur du même processus.

## Ce qu'elle sait faire

1. **Détection des fonctionnalités CPU à l'exécution** — `open_cpu::detect()`
   renvoie un `&'static CpuCapabilities`. Elle utilise
   `std::is_x86_feature_detected!` et met en cache le résultat de la première
   détection dans un `OnceLock`, si bien que le coût des appels suivants est
   quasi nul.
2. **Dispatch à l'exécution des opérations RAID6 GF(2^8)** — selon le résultat
   de la détection, l'implémentation scalaire / PCLMULQDQ / AVX2 / AVX-512 est
   choisie à l'exécution.

## Utilisation

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

### Liste des API publiques

| API | Contenu | Dispatch |
|---|---|---|
| `detect() -> &'static CpuCapabilities` | Détection des fonctionnalités CPU (cache `OnceLock`) | — |
| `runtime_summary() -> String` | Résumé sur une ligne du résultat de détection + de l'implémentation choisie | — |
| `selected_impl() -> GfImpl` | Implémentation retenue pour les opérations GF | — |
| `gf_xor(dst, src)` | `dst ^= src` (parité P) | AVX2 / scalaire |
| `gf_mul_parity(dst, src, factor)` | `dst ^= src * factor` (parité Q) | AVX-512 (opt-in) / AVX2 / PCLMULQDQ / scalaire |
| `gf_mul_pow2_xor(acc, src, times)` | `acc = acc * 2^times ^ src` | AVX2 / scalaire |
| `gf_mul2_xor` / `gf_mul4_xor` | Les cas `times=1` / `times=2` de ce qui précède | idem |
| `raid6_parity(stripes, p, q)` | Calcul groupé de P/Q par table de coefficients | conforme à ce qui précède |
| `raid6_parity3(stripes, p, q, r)` | Calcul groupé de P/Q/R par la méthode de Horner | conforme à ce qui précède |
| `gf_mul(a, b) -> u8` | Multiplication GF sur un octet (`const fn`) | — |
| `gf_mul2_byte(b) -> u8` | Doublement sur GF d'un octet (`const fn`) | — |
| `raid6_coeff(i) -> u8` | Coefficient RAID6 `g^i` (`g = 2`) | — |

Des versions `*_scalar` / `*_avx2` / `*_pclmul` / `*_avx512` appelant
explicitement chaque implémentation sont également publiques (pour les
mesures et la vérification croisée ; les versions SIMD sont `unsafe`).

## Jeux d'instructions détectés

| Jeu d'instructions | Détection | Utilisation dans cette crate |
|---|---|---|
| SSE2 | ✅ | Utilisé en appui du chemin PCLMULQDQ |
| SSSE3 | ✅ | `pshufb` (réduction du chemin PCLMULQDQ) |
| PCLMULQDQ | ✅ | Multiplication GF(2^8) (implémentation par multiplication sans retenue) |
| AVX2 | ✅ | Multiplication GF(2^8) (split-table `vpshufb`, choisi par défaut) |
| AVX-512F / BW / VL | ✅ | Chemin de multiplication GF(2^8) présent (**exécution non vérifiée**, voir ci-dessous) |
| POPCNT | ✅ | Détection seulement (aucune implémentation l'utilisant) |
| BMI1 / BMI2 | ✅ | Détection seulement (aucune implémentation l'utilisant) |
| FMA3 | ✅ | Détection seulement (aucune implémentation l'utilisant) |
| AES-NI | ✅ | Détection seulement (aucune implémentation l'utilisant) |
| SHA-NI | ✅ | Détection seulement (aucune implémentation l'utilisant) |
| AVX-VNNI | ✅ | Détection seulement (en vue d'une future inférence IA, aucune implémentation l'utilisant) |
| AVX-512 VNNI | ✅ | Détection seulement (en vue d'une future inférence IA, aucune implémentation l'utilisant) |

Sur les architectures autres que x86/x86_64, tous les champs valent `false` et
le code retombe sur l'implémentation scalaire (la compilation passe).

## Détails de l'implémentation GF(2^8)

Le polynôme irréductible est `0x11d` (x^8+x^4+x^3+x^2+1) et le générateur est
`g = 2`. Identique à Linux md/RAID6 et à ZFS RAID-Z.

- **Scalaire** : implémentation de référence par table nibble split
  (16 entrées × 2).
- **PCLMULQDQ** : en étalant chaque octet à intervalle de 16 bits, le produit
  sans retenue avec un coefficient de 8 bits (15 bits au maximum) ne déborde
  pas sur l'emplacement voisin. En exploitant cette propriété, 4 octets sont
  multipliés en une seule instruction, puis réduits vers GF(2^8) avec deux
  `pshufb`. 16 octets/itération.
- **AVX2** : implémentation split-table par `vpshufb`. 32 octets/itération.
- **AVX-512F/BW** : même split-table traitée à 64 octets/itération.

## Benchmarks mesurés

`cargo run --release --example bench` (4 MiB × 50 fois = 200 MiB, factor=0x8d)

Machine de développement : **AMD Ryzen 9 3950X** / Windows 11 / rustc 1.96.0

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

**À propos de la dispersion des mesures (relevé honnête)** : sur quatre
exécutions consécutives, le facteur AVX2 de la multiplication GF a varié entre
**11,6 et 18,1×**, la méthode de Horner entre **2,70 et 3,52×** et le XOR entre
**1,12 et 1,25×** (le côté scalaire restant stable autour de 207 ms). Comme le
côté SIMD atteint 10 000 à 20 000 MiB/s, il est **limité par la bande passante
mémoire** et donc sensible à l'état du cache et aux autres processus. Le
tableau ci-dessus reproduit tel quel le résultat d'une seule exécution : il est
plus exact d'appréhender les facteurs comme des plages.

- **La multiplication GF(2^8) par un coefficient arbitraire est 11,6 à 18,1×
  plus rapide en AVX2 qu'en scalaire (mesuré)**. Cela profite à la parité Q de
  RAID6 et aux chemins de reconstruction.
- **La méthode de Horner est 2,70 à 3,52× plus rapide (mesuré)**. Comme la
  version scalaire est déjà optimisée par des astuces de bits sur u64, l'écart
  n'est pas aussi grand que pour la multiplication GF.
- **Le simple XOR est 1,12 à 1,25× plus rapide**. Dès la version scalaire (u64)
  on est collé à la bande passante mémoire, donc la marge offerte par la
  vectorisation est faible (comme prévu).
- PCLMULQDQ atteint 1,90× et n'a d'intérêt que comme **repli pour les vieux
  CPU où AVX2 n'est pas disponible**.

## État de la vérification (divulgation honnête)

- ✅ **Scalaire / PCLMULQDQ / AVX2** : vérifiés à l'exécution sur la machine de
  développement ci-dessus. Avec `cargo test`, la justesse de l'implémentation
  scalaire est contrôlée par rapport à une implémentation naïve à décalages de
  bits, puis la concordance des sorties de l'implémentation PCLMULQDQ
  (**l'ensemble des 256 coefficients × 9 longueurs différentes**) et de
  l'implémentation AVX2 (8 coefficients × 11 longueurs, traitement des restes
  inclus) est vérifiée par rapport à l'implémentation scalaire. Les 15 tests et
  les 2 doctests passent.
- ⚠️ **Le chemin AVX-512 n'est pas vérifié à l'exécution.** La machine de
  développement (Ryzen 9 3950X) n'ayant pas AVX-512, **seul le fait que la
  compilation passe** a été vérifié. Par sécurité, il n'est pas retenu par le
  dispatch par défaut et ne s'active qu'en opt-in, lorsque la variable
  d'environnement `OPEN_CPU_ENABLE_AVX512=1` est définie. Ce traitement sera
  maintenu jusqu'à ce que la vérification soit faite sur une machine dotée
  d'AVX-512.
- ⚠️ POPCNT/BMI1/BMI2/FMA/AES-NI/SHA-NI/VNNI **ne disposent que d'un champ de
  détection** ; aucune implémentation de calcul les utilisant n'existe encore.

## Exécution des tests et des benchmarks

```
cargo build --release
cargo test --release
cargo run --release --example bench
```

## Adoptions effectives (au 2026-08-22)

| Dépôt | Utilisation |
|---|---|
| [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z) | La détection des fonctionnalités CPU, la multiplication GF(2^8), le XOR et la méthode de Horner (chemin AVX2) de `zfs_accel_hlsl/src/simd.rs` sont délégués à cette crate. Après la migration, les 39 tests existants passent tous et les valeurs numériques concordent parfaitement. |
| [`open-english`](https://github.com/aon-co-jp/open-english) | Résumé sur une ligne dans le journal de démarrage du serveur, et `GET /v1/cpu-runtime` (renvoie en JSON les jeux d'instructions CPU de la plateforme d'exécution). |

Non encore adoptée (cibles futures) : `aruaru-db` (sommes de contrôle,
compression), `aruaru-llm` (calcul matriciel), `open-cuda` (repli CPU en
l'absence de GPU).

## Liens connexes

- Procédure de migration : [PORTING.md](PORTING-France.md)
- Politique de développement et HANDOFF : [CLAUDE.md](CLAUDE-France.md)
- Organisation GitHub : https://github.com/aon-co-jp
