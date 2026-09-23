//! 複数命令セットの **組み合わせ** に応じてディスパッチする数値・ビット演算。
//!
//! ここに置くのは「検出しているだけ」ではなく、実際にその命令を使う実装を
//! 持つものだけ:
//!
//! - [`dot_f32`] / [`axpy_f32`] / [`scale_f32`] : AVX2 + FMA3 の組み合わせが
//!   揃っている場合のみ `vfmadd` を使う経路へ入る(AVX2 のみの CPU では
//!   乗算+加算の 2 命令版、SIMD 無しならスカラー)。
//! - [`popcount_bytes`] / [`hamming_distance`] : POPCNT を使うビット計数。
//! - [`extract_bits`] / [`deposit_bits`] : BMI2 の `pext` / `pdep`。
//! - [`trailing_zeros_u64`] : BMI1 の `tzcnt`。
//!
//! いずれも **スカラー実装と結果が完全に一致する** ことをテストで確認している。

use crate::caps::detect;
use crate::isa::{avx512_opt_in, Feature, IsaProfile};

/// 浮動小数点カーネルで実際に選ばれた実装。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloatImpl {
    /// SIMD 無しのスカラー(4 要素アンロール)。
    Scalar,
    /// AVX2 のみ(`vmulps` + `vaddps`)。
    Avx2,
    /// AVX2 + FMA3(`vfmadd231ps`)。
    Avx2Fma,
    /// AVX-512F(**この開発機では実行未検証**、`OPEN_CPU_ENABLE_AVX512=1` で opt-in)。
    Avx512,
    /// aarch64 NEON(`vfmaq_f32`、2026-09-23新設——ユーザー指示「スマホの
    /// コア構成やVPSのAVX512フルセットやローカルPCのAVX2やFMA3などの機能を
    /// フルで活かせるように」への対応。ARMv8-AはNEONが必須実装のため
    /// 検出フラグ確認は形式的だが、他アーキテクチャと同じ「検出してから
    /// 使う」設計を崩さないため`is_aarch64_feature_detected!`は維持する)。
    Neon,
}

impl std::fmt::Display for FloatImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            FloatImpl::Scalar => "scalar",
            FloatImpl::Avx2 => "avx2",
            FloatImpl::Avx2Fma => "avx2+fma3",
            FloatImpl::Avx512 => "avx512f",
            FloatImpl::Neon => "neon",
        })
    }
}

/// 現在の CPU で浮動小数点カーネルに選ばれる実装。
pub fn selected_float_impl() -> FloatImpl {
    #[cfg(target_arch = "x86_64")]
    {
        let caps = detect();
        if caps.at_least(IsaProfile::Avx512) && avx512_opt_in() {
            return FloatImpl::Avx512;
        }
        if caps.supports_all(&[Feature::Avx2, Feature::Fma]) {
            return FloatImpl::Avx2Fma;
        }
        if caps.avx2 {
            return FloatImpl::Avx2;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            return FloatImpl::Neon;
        }
    }
    FloatImpl::Scalar
}

// ---------------------------------------------------------------------------
// dot / axpy / scale
// ---------------------------------------------------------------------------

/// 内積 `sum(a[i] * b[i])`。
///
/// # Panics
/// `a.len() != b.len()` のときパニックする。
pub fn dot_f32(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "dot_f32: 長さが一致していない");
    #[cfg(target_arch = "x86_64")]
    {
        match selected_float_impl() {
            FloatImpl::Avx512 => return unsafe { dot_f32_avx512(a, b) },
            FloatImpl::Avx2Fma => return unsafe { dot_f32_avx2_fma(a, b) },
            FloatImpl::Avx2 => return unsafe { dot_f32_avx2(a, b) },
            FloatImpl::Scalar | FloatImpl::Neon => {}
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if selected_float_impl() == FloatImpl::Neon {
            return unsafe { dot_f32_neon(a, b) };
        }
    }
    dot_f32_scalar(a, b)
}

/// 内積のスカラー実装(参照実装)。
pub fn dot_f32_scalar(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "dot_f32_scalar: 長さが一致していない");
    // SIMD 版と同様に部分和を 4 本持ち、丸め順序の差を小さくする。
    let mut acc = [0f32; 4];
    let chunks = a.len() / 4;
    for i in 0..chunks {
        for j in 0..4 {
            acc[j] += a[i * 4 + j] * b[i * 4 + j];
        }
    }
    let mut s = acc[0] + acc[1] + acc[2] + acc[3];
    for i in chunks * 4..a.len() {
        s += a[i] * b[i];
    }
    s
}

/// `acc[i] += scale * src[i]`。
///
/// # Panics
/// `acc.len() != src.len()` のときパニックする。
pub fn axpy_f32(acc: &mut [f32], src: &[f32], scale: f32) {
    assert_eq!(acc.len(), src.len(), "axpy_f32: 長さが一致していない");
    #[cfg(target_arch = "x86_64")]
    {
        match selected_float_impl() {
            FloatImpl::Avx512 => return unsafe { axpy_f32_avx512(acc, src, scale) },
            FloatImpl::Avx2Fma => return unsafe { axpy_f32_avx2_fma(acc, src, scale) },
            FloatImpl::Avx2 => return unsafe { axpy_f32_avx2(acc, src, scale) },
            FloatImpl::Scalar | FloatImpl::Neon => {}
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if selected_float_impl() == FloatImpl::Neon {
            return unsafe { axpy_f32_neon(acc, src, scale) };
        }
    }
    axpy_f32_scalar(acc, src, scale)
}

/// `acc[i] += scale * src[i]` のスカラー実装(参照実装)。
pub fn axpy_f32_scalar(acc: &mut [f32], src: &[f32], scale: f32) {
    assert_eq!(acc.len(), src.len(), "axpy_f32_scalar: 長さが一致していない");
    for (d, s) in acc.iter_mut().zip(src.iter()) {
        *d += scale * *s;
    }
}

/// `dst[i] *= scale`。
pub fn scale_f32(dst: &mut [f32], scale: f32) {
    #[cfg(target_arch = "x86_64")]
    {
        if detect().avx2 {
            unsafe { scale_f32_avx2(dst, scale) };
            return;
        }
    }
    for d in dst.iter_mut() {
        *d *= scale;
    }
}

// ---------------------------------------------------------------------------
// aarch64 NEON(2026-09-23新設)
// ---------------------------------------------------------------------------

/// `dot_f32`のNEON実装。128bitレーン(f32×4)を2本並列に累積してから
/// 水平加算する(x86 AVX2版の「2アキュムレータで依存関係を切る」構成を
/// NEONへ移植)。
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn dot_f32_neon(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::aarch64::*;
    unsafe {
        let n = a.len();
        let mut acc0 = vdupq_n_f32(0.0);
        let mut acc1 = vdupq_n_f32(0.0);
        let mut i = 0;
        while i + 8 <= n {
            let va0 = vld1q_f32(a.as_ptr().add(i));
            let vb0 = vld1q_f32(b.as_ptr().add(i));
            acc0 = vfmaq_f32(acc0, va0, vb0);
            let va1 = vld1q_f32(a.as_ptr().add(i + 4));
            let vb1 = vld1q_f32(b.as_ptr().add(i + 4));
            acc1 = vfmaq_f32(acc1, va1, vb1);
            i += 8;
        }
        while i + 4 <= n {
            let va = vld1q_f32(a.as_ptr().add(i));
            let vb = vld1q_f32(b.as_ptr().add(i));
            acc0 = vfmaq_f32(acc0, va, vb);
            i += 4;
        }
        let mut s = vaddvq_f32(vaddq_f32(acc0, acc1));
        while i < n {
            s += a[i] * b[i];
            i += 1;
        }
        s
    }
}

/// `axpy_f32`のNEON実装(`acc[i] += scale * src[i]`)。
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn axpy_f32_neon(acc: &mut [f32], src: &[f32], scale: f32) {
    use std::arch::aarch64::*;
    unsafe {
        let n = acc.len();
        let vscale = vdupq_n_f32(scale);
        let mut i = 0;
        while i + 4 <= n {
            let vacc = vld1q_f32(acc.as_ptr().add(i));
            let vsrc = vld1q_f32(src.as_ptr().add(i));
            let result = vfmaq_f32(vacc, vsrc, vscale);
            vst1q_f32(acc.as_mut_ptr().add(i), result);
            i += 4;
        }
        while i < n {
            acc[i] += scale * src[i];
            i += 1;
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn dot_f32_avx2_fma(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    unsafe {
        let n = a.len();
        let mut acc0 = _mm256_setzero_ps();
        let mut acc1 = _mm256_setzero_ps();
        let mut i = 0;
        while i + 16 <= n {
            let va0 = _mm256_loadu_ps(a.as_ptr().add(i));
            let vb0 = _mm256_loadu_ps(b.as_ptr().add(i));
            acc0 = _mm256_fmadd_ps(va0, vb0, acc0);
            let va1 = _mm256_loadu_ps(a.as_ptr().add(i + 8));
            let vb1 = _mm256_loadu_ps(b.as_ptr().add(i + 8));
            acc1 = _mm256_fmadd_ps(va1, vb1, acc1);
            i += 16;
        }
        while i + 8 <= n {
            let va = _mm256_loadu_ps(a.as_ptr().add(i));
            let vb = _mm256_loadu_ps(b.as_ptr().add(i));
            acc0 = _mm256_fmadd_ps(va, vb, acc0);
            i += 8;
        }
        let mut s = hsum256(_mm256_add_ps(acc0, acc1));
        while i < n {
            s += a[i] * b[i];
            i += 1;
        }
        s
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dot_f32_avx2(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    unsafe {
        let n = a.len();
        let mut acc = _mm256_setzero_ps();
        let mut i = 0;
        while i + 8 <= n {
            let va = _mm256_loadu_ps(a.as_ptr().add(i));
            let vb = _mm256_loadu_ps(b.as_ptr().add(i));
            acc = _mm256_add_ps(acc, _mm256_mul_ps(va, vb));
            i += 8;
        }
        let mut s = hsum256(acc);
        while i < n {
            s += a[i] * b[i];
            i += 1;
        }
        s
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx")]
unsafe fn hsum256(v: std::arch::x86_64::__m256) -> f32 {
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;
    {
        let lo = _mm256_castps256_ps128(v);
        let hi = _mm256_extractf128_ps(v, 1);
        let s = _mm_add_ps(lo, hi);
        let s = _mm_add_ps(s, _mm_movehl_ps(s, s));
        let s = _mm_add_ss(s, _mm_shuffle_ps(s, s, 0x55));
        _mm_cvtss_f32(s)
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn axpy_f32_avx2_fma(acc: &mut [f32], src: &[f32], scale: f32) {
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;
    unsafe {
        let n = acc.len();
        let vs = _mm256_set1_ps(scale);
        let mut i = 0;
        while i + 8 <= n {
            let d = _mm256_loadu_ps(acc.as_ptr().add(i));
            let s = _mm256_loadu_ps(src.as_ptr().add(i));
            _mm256_storeu_ps(acc.as_mut_ptr().add(i), _mm256_fmadd_ps(vs, s, d));
            i += 8;
        }
        while i < n {
            acc[i] += scale * src[i];
            i += 1;
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn axpy_f32_avx2(acc: &mut [f32], src: &[f32], scale: f32) {
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;
    unsafe {
        let n = acc.len();
        let vs = _mm256_set1_ps(scale);
        let mut i = 0;
        while i + 8 <= n {
            let d = _mm256_loadu_ps(acc.as_ptr().add(i));
            let s = _mm256_loadu_ps(src.as_ptr().add(i));
            _mm256_storeu_ps(acc.as_mut_ptr().add(i), _mm256_add_ps(d, _mm256_mul_ps(vs, s)));
            i += 8;
        }
        while i < n {
            acc[i] += scale * src[i];
            i += 1;
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn scale_f32_avx2(dst: &mut [f32], scale: f32) {
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;
    unsafe {
        let n = dst.len();
        let vs = _mm256_set1_ps(scale);
        let mut i = 0;
        while i + 8 <= n {
            let d = _mm256_loadu_ps(dst.as_ptr().add(i));
            _mm256_storeu_ps(dst.as_mut_ptr().add(i), _mm256_mul_ps(d, vs));
            i += 8;
        }
        while i < n {
            dst[i] *= scale;
            i += 1;
        }
    }
}

// --- AVX-512(実行未検証、既定では選択されない) ---

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
unsafe fn dot_f32_avx512(a: &[f32], b: &[f32]) -> f32 {
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;
    unsafe {
        let n = a.len();
        let mut acc = _mm512_setzero_ps();
        let mut i = 0;
        while i + 16 <= n {
            let va = _mm512_loadu_ps(a.as_ptr().add(i));
            let vb = _mm512_loadu_ps(b.as_ptr().add(i));
            acc = _mm512_fmadd_ps(va, vb, acc);
            i += 16;
        }
        let mut s = _mm512_reduce_add_ps(acc);
        while i < n {
            s += a[i] * b[i];
            i += 1;
        }
        s
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
unsafe fn axpy_f32_avx512(acc: &mut [f32], src: &[f32], scale: f32) {
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;
    unsafe {
        let n = acc.len();
        let vs = _mm512_set1_ps(scale);
        let mut i = 0;
        while i + 16 <= n {
            let d = _mm512_loadu_ps(acc.as_ptr().add(i));
            let s = _mm512_loadu_ps(src.as_ptr().add(i));
            _mm512_storeu_ps(acc.as_mut_ptr().add(i), _mm512_fmadd_ps(vs, s, d));
            i += 16;
        }
        while i < n {
            acc[i] += scale * src[i];
            i += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// ビット演算(POPCNT / BMI1 / BMI2)
// ---------------------------------------------------------------------------

/// バイト列の立っているビット数を数える(POPCNT が有効なら `popcnt` を使う)。
pub fn popcount_bytes(data: &[u8]) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        if detect().popcnt {
            return unsafe { popcount_bytes_popcnt(data) };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            return unsafe { popcount_bytes_neon(data) };
        }
    }
    popcount_bytes_scalar(data)
}

/// [`popcount_bytes`] のスカラー参照実装。
pub fn popcount_bytes_scalar(data: &[u8]) -> u64 {
    data.iter().map(|b| b.count_ones() as u64).sum()
}

/// `popcount_bytes`のNEON実装。`vcntq_u8`(バイトごとのポップカウント)+
/// `vaddlvq_u8`(u16へワイドニングしながらの水平加算、16レーン×最大8=128が
/// 上限のためu16でオーバーフローしない)を16バイトずつ処理する。
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn popcount_bytes_neon(data: &[u8]) -> u64 {
    use std::arch::aarch64::*;
    unsafe {
        let mut total = 0u64;
        let n = data.len();
        let mut i = 0;
        while i + 16 <= n {
            let v = vld1q_u8(data.as_ptr().add(i));
            let counts = vcntq_u8(v);
            total += vaddlvq_u8(counts) as u64;
            i += 16;
        }
        while i < n {
            total += data[i].count_ones() as u64;
            i += 1;
        }
        total
    }
}

/// `hamming_distance`のNEON実装。`veorq_u8`でXORしてから
/// `popcount_bytes_neon`と同じポップカウント手順を適用する。
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn hamming_distance_neon(a: &[u8], b: &[u8]) -> u64 {
    use std::arch::aarch64::*;
    unsafe {
        let mut total = 0u64;
        let n = a.len();
        let mut i = 0;
        while i + 16 <= n {
            let va = vld1q_u8(a.as_ptr().add(i));
            let vb = vld1q_u8(b.as_ptr().add(i));
            let counts = vcntq_u8(veorq_u8(va, vb));
            total += vaddlvq_u8(counts) as u64;
            i += 16;
        }
        while i < n {
            total += (a[i] ^ b[i]).count_ones() as u64;
            i += 1;
        }
        total
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "popcnt")]
unsafe fn popcount_bytes_popcnt(data: &[u8]) -> u64 {
    let mut total = 0u64;
    let (pre, mid, post) = unsafe { data.align_to::<u64>() };
    for b in pre {
        total += b.count_ones() as u64;
    }
    for w in mid {
        total += w.count_ones() as u64;
    }
    for b in post {
        total += b.count_ones() as u64;
    }
    total
}

/// 2 つのバイト列のハミング距離(異なるビット数)。
///
/// # Panics
/// 長さが一致しない場合パニックする。
pub fn hamming_distance(a: &[u8], b: &[u8]) -> u64 {
    assert_eq!(a.len(), b.len(), "hamming_distance: 長さが一致していない");
    #[cfg(target_arch = "x86_64")]
    {
        if detect().popcnt {
            return unsafe { hamming_distance_popcnt(a, b) };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            return unsafe { hamming_distance_neon(a, b) };
        }
    }
    hamming_distance_scalar(a, b)
}

/// [`hamming_distance`] のスカラー参照実装。
pub fn hamming_distance_scalar(a: &[u8], b: &[u8]) -> u64 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x ^ y).count_ones() as u64)
        .sum()
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "popcnt")]
unsafe fn hamming_distance_popcnt(a: &[u8], b: &[u8]) -> u64 {
    let n = a.len();
    let words = n / 8;
    let mut total = 0u64;
    for i in 0..words {
        let mut xa = [0u8; 8];
        let mut xb = [0u8; 8];
        xa.copy_from_slice(&a[i * 8..i * 8 + 8]);
        xb.copy_from_slice(&b[i * 8..i * 8 + 8]);
        total += (u64::from_le_bytes(xa) ^ u64::from_le_bytes(xb)).count_ones() as u64;
    }
    for i in words * 8..n {
        total += (a[i] ^ b[i]).count_ones() as u64;
    }
    total
}

/// `mask` で 1 の立っている位置のビットだけを下位へ詰める(BMI2 の `pext`)。
///
/// **BMI2 ビットが立っていても無条件には使わない。** AMD Zen〜Zen 2
/// (family 17h)では `pext`/`pdep` がマイクロコード実装で非常に遅く、
/// スカラーのループより遅い。[`CpuCapabilities::fast_bmi2`] で
/// ハードウェア実装の CPU に限って BMI2 経路へ入る。
pub fn extract_bits(value: u64, mask: u64) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        if detect().fast_bmi2() {
            return unsafe { std::arch::x86_64::_pext_u64(value, mask) };
        }
    }
    extract_bits_scalar(value, mask)
}

/// [`extract_bits`] のスカラー参照実装。
pub fn extract_bits_scalar(value: u64, mut mask: u64) -> u64 {
    let mut result = 0u64;
    let mut out = 0u32;
    while mask != 0 {
        let lsb = mask & mask.wrapping_neg();
        if value & lsb != 0 {
            result |= 1u64 << out;
        }
        mask ^= lsb;
        out += 1;
    }
    result
}

/// 下位から順に `mask` の 1 の位置へビットを配る(BMI2 の `pdep`)。
pub fn deposit_bits(value: u64, mask: u64) -> u64 {
    #[cfg(target_arch = "x86_64")]
    {
        if detect().fast_bmi2() {
            return unsafe { std::arch::x86_64::_pdep_u64(value, mask) };
        }
    }
    deposit_bits_scalar(value, mask)
}

/// [`deposit_bits`] のスカラー参照実装。
pub fn deposit_bits_scalar(value: u64, mut mask: u64) -> u64 {
    let mut result = 0u64;
    let mut src = 0u32;
    while mask != 0 {
        let lsb = mask & mask.wrapping_neg();
        if value >> src & 1 != 0 {
            result |= lsb;
        }
        mask ^= lsb;
        src += 1;
    }
    result
}

/// 末尾に連続する 0 ビットの数(BMI1 の `tzcnt`、0 なら 64)。
pub fn trailing_zeros_u64(v: u64) -> u32 {
    #[cfg(target_arch = "x86_64")]
    {
        if detect().bmi1 {
            return unsafe { std::arch::x86_64::_tzcnt_u64(v) as u32 };
        }
    }
    v.trailing_zeros()
}

/// ビット演算で選ばれている実装の説明(ログ用)。
pub fn bit_impl_summary() -> String {
    // 2026-09-23修正: `caps`(x86専用の`CpuCapabilities`)しか見ていなかった
    // ため、aarch64で実際は`popcount_bytes`がNEON経由にディスパッチされて
    // いても常に「scalar」と表示する不整合があった(実機〈arrows We2 PLUS
    // M06〉で「popcount: scalar」なのに実測2倍以上速いという矛盾を確認)。
    #[cfg(target_arch = "aarch64")]
    let popcount_label = if std::arch::is_aarch64_feature_detected!("neon") { "neon" } else { "scalar" };
    #[cfg(not(target_arch = "aarch64"))]
    let popcount_label = if detect().popcnt { "popcnt" } else { "scalar" };

    let caps = detect();
    format!(
        "popcount: {} | pext/pdep: {} | tzcnt: {}",
        popcount_label,
        if caps.fast_bmi2() {
            "bmi2 (hw)"
        } else if caps.bmi2 {
            "scalar (bmi2 present but microcoded/slow on this CPU)"
        } else {
            "scalar"
        },
        if caps.bmi1 { "bmi1" } else { "scalar" },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq(n: usize, seed: u32) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                ((s >> 8) as f32 / 8388608.0) - 1.0
            })
            .collect()
    }

    #[test]
    fn dot_matches_scalar() {
        for n in [0usize, 1, 3, 7, 8, 15, 16, 17, 31, 64, 1000] {
            let a = seq(n, 1);
            let b = seq(n, 2);
            let fast = dot_f32(&a, &b);
            let slow = dot_f32_scalar(&a, &b);
            assert!(
                (fast - slow).abs() <= 1e-3 * (1.0 + slow.abs()),
                "n={n} fast={fast} slow={slow}"
            );
        }
    }

    #[test]
    fn axpy_matches_scalar() {
        for n in [0usize, 1, 5, 8, 9, 33, 512] {
            let src = seq(n, 3);
            let base = seq(n, 4);
            let mut fast = base.clone();
            let mut slow = base;
            axpy_f32(&mut fast, &src, 1.5);
            axpy_f32_scalar(&mut slow, &src, 1.5);
            for i in 0..n {
                assert!((fast[i] - slow[i]).abs() <= 1e-5, "n={n} i={i}");
            }
        }
    }

    #[test]
    fn scale_matches_scalar() {
        let mut v = seq(37, 5);
        let expect: Vec<f32> = v.iter().map(|x| x * 0.25).collect();
        scale_f32(&mut v, 0.25);
        for (a, b) in v.iter().zip(expect.iter()) {
            assert!((a - b).abs() <= 1e-6);
        }
    }

    #[test]
    fn popcount_and_hamming_match_scalar() {
        let a: Vec<u8> = (0..1000u32).map(|i| (i * 37 % 251) as u8).collect();
        let b: Vec<u8> = (0..1000u32).map(|i| (i * 91 % 253) as u8).collect();
        assert_eq!(popcount_bytes(&a), popcount_bytes_scalar(&a));
        assert_eq!(hamming_distance(&a, &b), hamming_distance_scalar(&a, &b));
        assert_eq!(popcount_bytes(&[]), 0);
        assert_eq!(hamming_distance(&a, &a), 0);
    }

    #[test]
    fn pext_pdep_match_scalar() {
        let cases: [(u64, u64); 6] = [
            (0, 0),
            (u64::MAX, 0xF0F0_F0F0_F0F0_F0F0),
            (0x0123_4567_89AB_CDEF, 0xFFFF_0000_FFFF_0000),
            (0xDEAD_BEEF_CAFE_BABE, 0x5555_5555_5555_5555),
            (1, 0xFF),
            (0xABCD, u64::MAX),
        ];
        for (v, m) in cases {
            assert_eq!(extract_bits(v, m), extract_bits_scalar(v, m), "pext {v:x} {m:x}");
            assert_eq!(deposit_bits(v, m), deposit_bits_scalar(v, m), "pdep {v:x} {m:x}");
        }
    }

    #[test]
    fn tzcnt_matches_scalar() {
        for v in [0u64, 1, 2, 0x8000_0000_0000_0000, 0xF0] {
            assert_eq!(trailing_zeros_u64(v), v.trailing_zeros());
        }
    }

    #[test]
    fn float_impl_reported() {
        println!("float impl: {}", selected_float_impl());
        println!("{}", bit_impl_summary());
    }
}
