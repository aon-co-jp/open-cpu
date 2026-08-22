> מקור יפני / 日本語原文: [README.md](../README.md)

# open-cpu

**ספריית זיהוי ערכות פקודות CPU ושיגור (dispatch) בזמן ריצה** (Rust), משותפת
לכל האקוסיסטם של `aon-co-jp`.

נוצרה כדי למנוע כפילות שבה `open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda`
כותבים כל אחד בנפרד קוד לזיהוי יכולות CPU.

**אין מדובר בשירות רקע (דימון).** זו קרייט ספרייה רגילה שכל מאגר מוסיף
ל-`[dependencies]` שב-`Cargo.toml` ומקשר לתוך אותו תהליך עצמו.

## מה היא יודעת לעשות

1. **זיהוי יכולות CPU בזמן ריצה** — `open_cpu::detect()` מחזירה
   `&'static CpuCapabilities`. היא משתמשת ב-`std::is_x86_feature_detected!`
   ושומרת את תוצאת הזיהוי הראשונה ב-`OnceLock`, כך שהעלות של קריאות חוזרות
   קרובה לאפס.
2. **שיגור בזמן ריצה של חישובי RAID6 GF(2^8)** — בהתאם לתוצאת הזיהוי נבחרת
   בזמן ריצה מימוש סקלרי / PCLMULQDQ / AVX2 / AVX-512.

## אופן השימוש

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

### רשימת ה-API הציבורי

| API | תוכן | שיגור |
|---|---|---|
| `detect() -> &'static CpuCapabilities` | זיהוי יכולות CPU (מטמון `OnceLock`) | — |
| `runtime_summary() -> String` | סיכום בשורה אחת של תוצאת הזיהוי + המימוש שנבחר | — |
| `selected_impl() -> GfImpl` | המימוש שנבחר עבור חישובי GF | — |
| `gf_xor(dst, src)` | `dst ^= src` (פריטי P) | AVX2 / סקלרי |
| `gf_mul_parity(dst, src, factor)` | `dst ^= src * factor` (פריטי Q) | AVX-512 (opt-in) / AVX2 / PCLMULQDQ / סקלרי |
| `gf_mul_pow2_xor(acc, src, times)` | `acc = acc * 2^times ^ src` | AVX2 / סקלרי |
| `gf_mul2_xor` / `gf_mul4_xor` | הנ"ל עם `times=1` / `times=2` | כנ"ל |
| `raid6_parity(stripes, p, q)` | חישוב מרוכז של P/Q בשיטת טבלת מקדמים | בהתאם לנ"ל |
| `raid6_parity3(stripes, p, q, r)` | חישוב מרוכז של P/Q/R בשיטת הורנר | בהתאם לנ"ל |
| `gf_mul(a, b) -> u8` | כפל GF של בית בודד (`const fn`) | — |
| `gf_mul2_byte(b) -> u8` | הכפלה פי 2 מעל GF של בית בודד (`const fn`) | — |
| `raid6_coeff(i) -> u8` | מקדם RAID6 `g^i` (`g = 2`) | — |

מפורסמות גם גרסאות `*_scalar` / `*_avx2` / `*_pclmul` / `*_avx512` הקוראות
לכל מימוש במפורש (לצורכי בנצ'מרק ואימות הדדי; גרסאות ה-SIMD הן `unsafe`).

## ערכות הפקודות שמזוהות

| ערכת פקודות | זיהוי | שימוש בקרייט זו |
|---|---|---|
| SSE2 | ✅ | משמשת כעזר למסלול PCLMULQDQ |
| SSSE3 | ✅ | `pshufb` (רדוקציה במסלול PCLMULQDQ) |
| PCLMULQDQ | ✅ | כפל GF(2^8) (מימוש כפל נטול נשא) |
| AVX2 | ✅ | כפל GF(2^8) (`vpshufb` split-table, נבחר כברירת מחדל) |
| AVX-512F / BW / VL | ✅ | קיים מסלול לכפל GF(2^8) (**לא נבדק בהרצה**, ראו להלן) |
| POPCNT | ✅ | זיהוי בלבד (אין מימוש שמשתמש בו) |
| BMI1 / BMI2 | ✅ | זיהוי בלבד (אין מימוש שמשתמש בו) |
| FMA3 | ✅ | זיהוי בלבד (אין מימוש שמשתמש בו) |
| AES-NI | ✅ | זיהוי בלבד (אין מימוש שמשתמש בו) |
| SHA-NI | ✅ | זיהוי בלבד (אין מימוש שמשתמש בו) |
| AVX-VNNI | ✅ | זיהוי בלבד (לטובת הסקת AI בעתיד, אין מימוש שמשתמש בו) |
| AVX-512 VNNI | ✅ | זיהוי בלבד (לטובת הסקת AI בעתיד, אין מימוש שמשתמש בו) |

בארכיטקטורות שאינן x86/x86_64 כל השדות יהיו `false` והמימוש ייפול חזרה
לסקלרי (הבנייה עוברת).

## פרטי מימוש GF(2^8)

הפולינום האי-פריק הוא `0x11d` (x^8+x^4+x^3+x^2+1), והיוצר הוא `g = 2`.
זהה ל-Linux md/RAID6 ול-ZFS RAID-Z.

- **סקלרי**: מימוש ייחוס באמצעות טבלת nibble split (16 ערכים × 2).
- **PCLMULQDQ**: כאשר פורשים כל בית במרווחים של 16 סיביות, המכפלה נטולת הנשא
  עם מקדם של 8 סיביות (עד 15 סיביות) אינה גולשת אל המשבצת הסמוכה. תכונה זו
  מנוצלת לכפל של 4 בתים יחד בפקודה אחת, ולאחר מכן רדוקציה ל-GF(2^8) בעזרת
  שתי פעולות `pshufb`. 16 byte/iter.
- **AVX2**: מימוש split-table באמצעות `vpshufb`. 32 byte/iter.
- **AVX-512F/BW**: אותה split-table בעיבוד של 64 byte/iter.

## בנצ'מרק מדוד

`cargo run --release --example bench` (4 MiB × 50 回 = 200 MiB, factor=0x8d)

מכונת הפיתוח: **AMD Ryzen 9 3950X** / Windows 11 / rustc 1.96.0

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

**על הפיזור בערכים המדודים (תיעוד כן)**: בארבע הרצות רצופות, יחס ההאצה של
AVX2 בכפל GF השתנה בטווח של **11.6 עד 18.1 פי**, שיטת הורנר בטווח של
**2.70 עד 3.52 פי**, ו-XOR בטווח של **1.12 עד 1.25 פי** (הצד הסקלרי היה יציב
סביב 207 ms). צד ה-SIMD מגיע ל-10,000 עד 20,000 MiB/s ולכן **חסום ברוחב הפס
של הזיכרון**, ורגיש למצב המטמון ולתהליכים אחרים. הטבלה שלמעלה היא פשוט
תוצאת הרצה בודדת כמות שהיא, ונכון יותר להתייחס ליחסי ההאצה כטווחים.

- **כפל GF(2^8) במקדם שרירותי מהיר פי 11.6 עד 18.1 ב-AVX2 לעומת סקלרי (מדוד)**.
  משפיע על מסלול פריטי Q ומסלול השחזור של RAID6.
- **שיטת הורנר מהירה פי 2.70 עד 3.52 (מדוד)**. מכיוון שהגרסה הסקלרית כבר
  ממוטבת בטריקי סיביות של u64, ההפרש קטן מזה שבכפל GF.
- **XOR פשוט מהיר פי 1.12 עד 1.25**. כבר בגרסה הסקלרית (u64) הביצוע צמוד
  לרוחב הפס של הזיכרון, ולכן מרחב השיפור מ-SIMD קטן (כצפוי).
- PCLMULQDQ מגיע לפי 1.90, ויש לו משמעות רק **כנפילה אחורה עבור מעבדים ישנים
  שאינם תומכים ב-AVX2**.

## מצב האימות (גילוי כן)

- ✅ **סקלרי / PCLMULQDQ / AVX2**: אומתו בהרצה על מכונת הפיתוח שלעיל.
  ב-`cargo test` נבדקה נכונות המימוש הסקלרי מול מימוש נאיבי מבוסס הזזות סיביות,
  ולאחר מכן נבדקה התאמת הפלט של מימוש PCLMULQDQ (**כל 256 המקדמים × 9 אורכים**)
  ושל מימוש AVX2 (8 מקדמים × 11 אורכים, כולל טיפול בשארית) מול המימוש הסקלרי.
  כל 15 הבדיקות + 2 doctests עברו.
- ⚠️ **מסלול AVX-512 לא נבדק בהרצה**. מכיוון שמכונת הפיתוח (Ryzen 9 3950X)
  אינה כוללת AVX-512, אומת **רק שהקומפילציה עוברת**. למען הבטיחות הוא אינו
  נבחר בשיגור ברירת המחדל, ומופעל בהצטרפות מרצון (opt-in) רק כאשר מוגדר
  משתנה הסביבה `OPEN_CPU_ENABLE_AVX512=1`. יחס זה יישמר עד שיושלם אימות
  על מכונה עם AVX-512.
- ⚠️ עבור POPCNT/BMI1/BMI2/FMA/AES-NI/SHA-NI/VNNI **קיימים רק שדות זיהוי**,
  ועדיין אין מימושי חישוב המשתמשים בהם.

## הרצת בדיקות ובנצ'מרק

```
cargo build --release
cargo test --release
cargo run --release --example bench
```

## אימוץ בפועל (נכון ל-2026-08-22)

| מאגר | אופן השימוש |
|---|---|
| [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z) | זיהוי יכולות CPU, כפל GF(2^8), XOR ושיטת הורנר (מסלול AVX2) שב-`zfs_accel_hlsl/src/simd.rs` הועברו לקרייט זו. גם לאחר המעבר כל 39 הבדיקות הקיימות עברו, והערכים זהים לחלוטין. |
| [`open-english`](https://github.com/aon-co-jp/open-english) | סיכום בשורה אחת ביומן עליית השרת, ו-`GET /v1/cpu-runtime` (מחזיר ב-JSON את ערכות הפקודות של תשתית ההרצה). |

טרם אומצו (יעדים עתידיים): `aruaru-db` (סכומי ביקורת ודחיסה),
`aruaru-llm` (חישובי מטריצות), `open-cuda` (נפילה אחורה ל-CPU בהיעדר GPU).

## קשור

- נוהל המעבר: [PORTING.md](PORTING-Hebrew.md)
- מדיניות פיתוח ו-HANDOFF: [CLAUDE.md](CLAUDE-Hebrew.md)
- GitHub organization: https://github.com/aon-co-jp
