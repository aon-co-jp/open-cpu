> מקור יפני / 日本語原文: [PORTING.md](../PORTING.md)

# PORTING.md — נוהל המעבר ל-`open-cpu` ממאגרים אחרים

נוהל מעבר לריכוז ב-`open-cpu` של קוד זיהוי יכולות CPU וקוד חישובי GF(2^8)
שמוחזק בנפרד ב-`open-raid-z` / `aruaru-db` / `aruaru-llm` / `open-cuda` ואחרים.

## 0. הנחות יסוד

`open-cpu` היא **קרייט ספרייה**, ולא שירות רקע. אין צורך בתקשורת בין תהליכים
או בהפעלת תהליך נפרד; מספיק להוסיף תלות ל-`Cargo.toml` ולקרוא לפונקציות.

## 1. הוספת התלות

תחת כונן העבודה המקומי `F:\runo`, תלות path היא הפשוטה ביותר:

```toml
[dependencies]
open-cpu = { path = "../open-cpu" }
```

כשמשתמשים מתוך קרייט חבר בתוך workspace, רצוי לכתוב ב-`Cargo.toml` של שורש
ה-workspace

```toml
[workspace.dependencies]
open-cpu = { path = "../open-cpu" }
```

ובצד החבר לכתוב `open-cpu = { workspace = true }`.

אם בעתיד עוברים לתלות git:

```toml
open-cpu = { git = "https://github.com/aon-co-jp/open-cpu", branch = "main" }
```

שם הקרייט הוא `open-cpu` עם מקף, והנתיב שאליו מפנים מתוך Rust הוא `open_cpu`
עם קו תחתון.

## 2. החלפת קוד זיהוי יכולות ה-CPU

לפני המעבר (דפוס נפוץ במאגרים השונים):

```rust
static HAS_AVX2: OnceLock<bool> = OnceLock::new();
fn has_avx2() -> bool {
    *HAS_AVX2.get_or_init(|| is_x86_feature_detected!("avx2"))
}
```

אחרי המעבר:

```rust
if open_cpu::detect().avx2 { /* ... */ }
```

`detect()` כבר שומרת מטמון פנימי ב-`OnceLock`, ולכן אין צורך במטמון נוסף בצד
הקורא. היא מחזירה `&'static CpuCapabilities` ולכן גם לא מתרחשת הקצאה.

השדות הזמינים: `avx2` `avx512f` `avx512bw` `avx512vl` `pclmulqdq`
`bmi1` `bmi2` `fma` `aes` `popcnt` `sha` `sse2` `ssse3` `avx_vnni` `avx512vnni`.

## 3. החלפת חישובי GF(2^8) / פריטי (בעיקר `open-raid-z`)

הפולינום האי-פריק של `open-cpu` הוא `0x11d` והיוצר הוא `g = 2`, זהה
ל-Linux md/RAID6 ול-ZFS RAID-Z. **לפני המעבר יש לוודא בהכרח שהפולינום והיוצר
של המאגר שלכם תואמים לאלה.** אם הם שונים, הערכים לא יתאימו.

| צורה נפוצה לפני המעבר | אחרי המעבר |
|---|---|
| `for i in .. { p[i] ^= d[i] }` | `open_cpu::gf_xor(&mut p, &d)` |
| `for i in .. { q[i] ^= gf_mul(d[i], c) }` | `open_cpu::gf_mul_parity(&mut q, &d, c)` |
| `for i in .. { acc[i] = mul2(acc[i]) ^ d[i] }` | `open_cpu::gf_mul2_xor(&mut acc, &d)` |
| `for i in .. { acc[i] = mul4(acc[i]) ^ d[i] }` | `open_cpu::gf_mul4_xor(&mut acc, &d)` |
| שיטת הורנר עם `×2^n` במספר חזרות שרירותי | `open_cpu::gf_mul_pow2_xor(&mut acc, &d, n)` |
| `gf_mul(a: u8, b: u8) -> u8` עצמי | `open_cpu::gf_mul(a, b)` (`const fn`) |
| `mul2_byte(b) -> u8` עצמי | `open_cpu::gf_mul2_byte(b)` (`const fn`) |
| חישוב עצמי של מקדם `g^i` | `open_cpu::raid6_coeff(i)` |
| חישוב מרוכז של P/Q | `open_cpu::raid6_parity(&stripes, &mut p, &mut q)` |
| חישוב מרוכז של P/Q/R (שקול ל-RAID-Z3) | `open_cpu::raid6_parity3(&stripes, &mut p, &mut q, &mut r)` |

`gf_mul_parity` / `gf_xor` יגרמו ל-panic כאשר `dst.len() != src.len()`.
יש להשוות מראש את אורכי ה-stripe בצד הקורא.

כשרוצים לקרוא במפורש למימוש מסוים (למטרות בנצ'מרק או אימות):

- `open_cpu::gf_mul_parity_scalar(...)` / `gf_xor_scalar(...)` /
  `gf_mul_pow2_xor_scalar(...)` — safe
- `unsafe { open_cpu::gf_mul_parity_avx2(...) }` — הקורא מתחייב לתמיכה ב-AVX2
- `unsafe { open_cpu::gf_mul_parity_pclmul(...) }` — כנ"ל עבור SSSE3+PCLMULQDQ
- `unsafe { open_cpu::gf_mul_parity_avx512(...) }` — **לא נבדק בהרצה**

## 4. אימות לאחר המעבר (חובה)

1. `cargo build` — שפתרון התלויות והבנייה עוברים.
2. `cargo test` — **שכל הבדיקות הקיימות עוברות**. במיוחד יש לוודא שרצף
   הבתים של הפריטי זהה לחלוטין לפני ההחלפה ואחריה. אם בבדיקות הקיימות אין
   השוואה של ערכי הפריטי, יש להוסיף אותה בעת המעבר.
3. אם מדפיסים ליומן שורה אחת של `open_cpu::runtime_summary()`, אפשר יהיה
   לבדוק בדיעבד איזה מימוש נבחר על החומרה בפועל.

## 4.5 דוגמת מעבר בפועל (`open-raid-z`, 2026-08-22)

כנקודת ייחוס, להלן עיקרי מקרה המעבר הראשון:

- `std::is_x86_feature_detected!` שבתוך `detect_level()` הוחלף בהפניה אל
  `open_cpu::detect()` (טיפוס ה-enum הייחודי למאגר `SimdLevel` נשאר כמות
  שהוא, ורק **החומר שעל בסיסו מתקבלת ההחלטה** הועבר ל-open-cpu). כך אין
  צורך לשנות כלל את הקוד הקורא הקיים.
- רק **מסלול ה-AVX2** של `gf_mul_xor_into()` / `xor_into()` /
  `mul_pow2_xor_into()` הועבר ל-open-cpu; מסלול ה-AVX-512 (שגם בצד open-cpu
  אינו מאומת) ומסלול ה-SSE2 (שאין לו מימוש ב-open-cpu) נשארו במימוש שבמאגר
  עצמו. ההחלטה: **לא לבצע החלפה שמורידה ביצועים**.
- גרעיני SIMD שיצאו משימוש לא נמחקו, אלא הושארו במקומם עם
  `#[allow(dead_code)]`, כדי לשמש ייחוס לאימות הדדי מול צד open-cpu בעתיד.

מומלץ להתקדם בדרך הזאת: "לא להחליף הכול בבת אחת, אלא להעביר בהדרגה החל
מהחלקים ששקולים ואינם פוגעים בביצועים".

## 5. נקודות לתשומת לב

- **מסלול AVX-512 לא נבדק בהרצה** (מכונת הפיתוח אינה כוללת אותו). הוא אינו
  נבחר כברירת מחדל, ולכן המעבר לא ישנה את ההתנהגות. רק כאשר מאמתים על מכונה
  עם AVX-512 יש להגדיר `OPEN_CPU_ENABLE_AVX512=1`.
- POPCNT/BMI/FMA/AES-NI/SHA-NI/VNNI הם **בזיהוי בלבד**. עדיין אין ב-`open-cpu`
  מימושים של סכומי ביקורת, דחיסה או חישובי מטריצות המשתמשים בהם, ולכן עדיין
  אי אפשר להעביר את הקוד המתאים בצד `aruaru-db` / `aruaru-llm`
  (אפשר להעביר תחילה רק את חלק הזיהוי).
- מחוץ ל-x86 (ARM וכדומה) כל היכולות יהיו `false` והביצוע ייפול למימוש
  הסקלרי. בנייה צולבת עוברת, אך אין תמיכה באופטימיזציות כגון NEON.
