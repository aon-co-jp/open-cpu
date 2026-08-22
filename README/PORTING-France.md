> Original japonais / 日本語原文: [PORTING.md](../PORTING.md)

# PORTING.md — procédure de bascule d'autres dépôts vers `open-cpu`

Procédure de migration permettant de regrouper dans `open-cpu` le code de
détection des fonctionnalités CPU et le code de calcul GF(2^8) que
`open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` et d'autres possèdent
séparément.

## 0. Prérequis

`open-cpu` est une **crate de bibliothèque**, pas un service résident. Aucune
communication interprocessus ni lancement d'un autre processus n'est
nécessaire : il suffit d'ajouter la dépendance dans `Cargo.toml` et d'appeler
les fonctions.

## 1. Ajout de la dépendance

Sous le lecteur de travail local `F:\runo`, la dépendance path est la plus
simple :

```toml
[dependencies]
open-cpu = { path = "../open-cpu" }
```

Pour une utilisation depuis une crate membre d'un espace de travail, il est
préférable d'écrire dans le `Cargo.toml` racine de l'espace de travail

```toml
[workspace.dependencies]
open-cpu = { path = "../open-cpu" }
```

et de mettre `open-cpu = { workspace = true }` côté membre.

En cas de bascule future vers une dépendance git :

```toml
open-cpu = { git = "https://github.com/aon-co-jp/open-cpu", branch = "main" }
```

Le nom de la crate s'écrit avec un tiret, `open-cpu`, tandis que le chemin
utilisé pour la référencer depuis Rust s'écrit avec un tiret bas, `open_cpu`.

## 2. Remplacement du code de détection des fonctionnalités CPU

Avant migration (motif courant dans chaque dépôt) :

```rust
static HAS_AVX2: OnceLock<bool> = OnceLock::new();
fn has_avx2() -> bool {
    *HAS_AVX2.get_or_init(|| is_x86_feature_detected!("avx2"))
}
```

Après migration :

```rust
if open_cpu::detect().avx2 { /* ... */ }
```

`detect()` étant déjà mis en cache en interne par un `OnceLock`, il est inutile
de mettre encore en cache côté appelant. Comme il renvoie un
`&'static CpuCapabilities`, aucune allocation ne se produit non plus.

Champs disponibles : `avx2` `avx512f` `avx512bw` `avx512vl` `pclmulqdq`
`bmi1` `bmi2` `fma` `aes` `popcnt` `sha` `sse2` `ssse3` `avx_vnni` `avx512vnni`.

## 3. Remplacement des calculs GF(2^8) / de parité (principalement `open-raid-z`)

Le polynôme irréductible d'`open-cpu` est `0x11d` et son générateur `g = 2`,
identiques à Linux md/RAID6 et à ZFS RAID-Z. **Avant de migrer, vérifier
impérativement que le polynôme et le générateur de votre dépôt concordent.**
S'ils diffèrent, les valeurs numériques ne correspondront plus.

| Forme fréquente avant migration | Après migration |
|---|---|
| `for i in .. { p[i] ^= d[i] }` | `open_cpu::gf_xor(&mut p, &d)` |
| `for i in .. { q[i] ^= gf_mul(d[i], c) }` | `open_cpu::gf_mul_parity(&mut q, &d, c)` |
| `for i in .. { acc[i] = mul2(acc[i]) ^ d[i] }` | `open_cpu::gf_mul2_xor(&mut acc, &d)` |
| `for i in .. { acc[i] = mul4(acc[i]) ^ d[i] }` | `open_cpu::gf_mul4_xor(&mut acc, &d)` |
| Méthode de Horner en `×2^n` avec un nombre quelconque de répétitions | `open_cpu::gf_mul_pow2_xor(&mut acc, &d, n)` |
| `gf_mul(a: u8, b: u8) -> u8` maison | `open_cpu::gf_mul(a, b)` (`const fn`) |
| `mul2_byte(b) -> u8` maison | `open_cpu::gf_mul2_byte(b)` (`const fn`) |
| Calcul maison du coefficient `g^i` | `open_cpu::raid6_coeff(i)` |
| Calcul groupé de P/Q | `open_cpu::raid6_parity(&stripes, &mut p, &mut q)` |
| Calcul groupé de P/Q/R (équivalent RAID-Z3) | `open_cpu::raid6_parity3(&stripes, &mut p, &mut q, &mut r)` |

`gf_mul_parity` / `gf_xor` provoquent une panique si
`dst.len() != src.len()`. L'appelant doit avoir aligné les longueurs de
stripes au préalable.

Pour appeler explicitement une implémentation précise (à des fins de mesure ou
de vérification) :

- `open_cpu::gf_mul_parity_scalar(...)` / `gf_xor_scalar(...)` /
  `gf_mul_pow2_xor_scalar(...)` — safe
- `unsafe { open_cpu::gf_mul_parity_avx2(...) }` — l'appelant garantit la prise
  en charge d'AVX2
- `unsafe { open_cpu::gf_mul_parity_pclmul(...) }` — idem pour SSSE3+PCLMULQDQ
- `unsafe { open_cpu::gf_mul_parity_avx512(...) }` — **exécution non vérifiée**

## 4. Vérification après migration (obligatoire)

1. `cargo build` — la résolution des dépendances et la compilation doivent
   passer.
2. `cargo test` — **tous les tests existants doivent passer**. Vérifier en
   particulier que les octets de parité concordent parfaitement avant et après
   le remplacement. Si les tests existants ne comparent pas les valeurs de
   parité, il faut les ajouter lors de la migration.
3. Émettre une ligne `open_cpu::runtime_summary()` dans le journal permet de
   vérifier après coup quelle implémentation a été retenue sur la machine
   réelle.

## 4.5 Exemple concret de migration (`open-raid-z`, 2026-08-22)

À titre de référence, voici les points clés du premier cas de migration :

- Le `std::is_x86_feature_detected!` figurant dans `detect_level()` a été
  remplacé par une consultation d'`open_cpu::detect()` (l'énumération
  `SimdLevel`, propre au dépôt, a été conservée telle quelle et **seuls les
  éléments servant à la décision** ont été transférés vers open-cpu). Aucun
  changement n'a été nécessaire côté appelants existants.
- **Seul le chemin AVX2** de `gf_mul_xor_into()` / `xor_into()` /
  `mul_pow2_xor_into()` a été délégué à open-cpu ; le chemin AVX-512 (non
  vérifié côté open-cpu également) et le chemin SSE2 (absent d'open-cpu) ont
  gardé l'implémentation du dépôt. C'est la décision de **ne pas faire de
  remplacement qui dégrade les performances**.
- Les noyaux SIMD devenus inutilisés n'ont pas été supprimés : ils ont été
  laissés en place avec `#[allow(dead_code)]` pour servir de référence lors
  d'une future vérification croisée avec le côté open-cpu.

Cette démarche consistant à « ne pas tout remplacer d'un coup, mais déléguer
progressivement en partant des parties équivalentes et sans perte de
performance » est recommandée.

## 5. Points d'attention

- **Le chemin AVX-512 n'est pas vérifié à l'exécution** (la machine de
  développement n'en dispose pas). Comme il n'est pas retenu par défaut, la
  migration ne modifie pas le comportement. Ne définir
  `OPEN_CPU_ENABLE_AVX512=1` que pour effectuer une vérification sur une
  machine AVX-512.
- POPCNT/BMI/FMA/AES-NI/SHA-NI/VNNI sont **seulement détectés**. `open-cpu` ne
  contient pas encore d'implémentation de sommes de contrôle, de compression ou
  de calcul matriciel les utilisant, si bien que le code correspondant côté
  `aruaru-db` / `aruaru-llm` ne peut pas encore être migré (migrer d'abord la
  seule partie détection reste possible).
- Hors x86 (ARM, etc.), toutes les fonctionnalités valent `false` et le code
  retombe sur l'implémentation scalaire. La compilation croisée passe, mais les
  optimisations de type NEON ne sont pas prises en charge.
