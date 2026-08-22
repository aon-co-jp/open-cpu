//! RAID6 で用いる GF(2^8) 有限体演算(既約多項式 0x11d = x^8+x^4+x^3+x^2+1)。
//!
//! Linux md/RAID6 および ZFS RAID-Z と同じ多項式を採用している。
//!
//! 提供する演算:
//! - [`gf_xor`]        : `dst ^= src`(P パリティ)
//! - [`gf_mul_parity`] : `dst ^= src * factor`(Q パリティ、GF(2^8) 乗算)
//!
//! 実装は [`crate::detect`] の結果に応じて実行時ディスパッチする。
//! 対応状況は [`GfImpl`] を参照。

use crate::caps::detect;

/// GF(2^8) の既約多項式(下位 8bit 表現)。
pub const GF_POLY: u16 = 0x11d;

/// 実際に選択された実装。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GfImpl {
    /// テーブル参照によるスカラー実装(全アーキテクチャ)。
    Scalar,
    /// SSSE3 + PCLMULQDQ によるキャリーレス乗算実装(16 byte/iter)。
    Pclmul,
    /// AVX2 の `vpshufb` split-table 実装(32 byte/iter)。
    Avx2,
    /// AVX-512F/BW の `vpshufb` split-table 実装(64 byte/iter)。
    ///
    /// **未検証**: 開発機(AMD Ryzen 9 3950X)は AVX-512 非搭載のため、
    /// このコードパスはコンパイル確認のみで実行検証されていない。
    Avx512,
}

// ---------------------------------------------------------------------------
// スカラー基礎演算
// ---------------------------------------------------------------------------

/// GF(2^8) の 1 バイト乗算(スカラー、ビットシフト法)。
pub const fn gf_mul(a: u8, b: u8) -> u8 {
    let mut a = a;
    let mut b = b;
    let mut r: u8 = 0;
    while b != 0 {
        if b & 1 != 0 {
            r ^= a;
        }
        let hi = a & 0x80;
        a <<= 1;
        if hi != 0 {
            a ^= 0x1d;
        }
        b >>= 1;
    }
    r
}

/// 16bit のキャリーレス積を GF(2^8) へ還元する(テーブル生成用)。
const fn gf_reduce16(x: u16) -> u8 {
    let mut v = x as u32;
    let mut i = 15;
    while i >= 8 {
        if v & (1 << i) != 0 {
            v ^= (GF_POLY as u32) << (i - 8);
        }
        i -= 1;
    }
    v as u8
}

/// `factor` 用の nibble split テーブル(low nibble 用 / high nibble 用)。
#[derive(Clone, Copy)]
struct SplitTable {
    lo: [u8; 16],
    hi: [u8; 16],
}

fn split_table(factor: u8) -> SplitTable {
    let mut t = SplitTable {
        lo: [0u8; 16],
        hi: [0u8; 16],
    };
    let mut i = 0usize;
    while i < 16 {
        t.lo[i] = gf_mul(i as u8, factor);
        t.hi[i] = gf_mul((i as u8) << 4, factor);
        i += 1;
    }
    t
}

/// PCLMULQDQ 実装で使う還元テーブル。
/// `RED_LO[n] = reduce(n << 8)`, `RED_HI[n] = reduce(n << 12)`
const RED_LO: [u8; 16] = {
    let mut t = [0u8; 16];
    let mut i = 0;
    while i < 16 {
        t[i] = gf_reduce16((i as u16) << 8);
        i += 1;
    }
    t
};
const RED_HI: [u8; 16] = {
    let mut t = [0u8; 16];
    let mut i = 0;
    while i < 16 {
        t[i] = gf_reduce16((i as u16) << 12);
        i += 1;
    }
    t
};

// ---------------------------------------------------------------------------
// 公開 API
// ---------------------------------------------------------------------------

/// このプロセスで [`gf_mul_parity`] が選択する実装を返す。
pub fn selected_impl() -> GfImpl {
    let c = detect();
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        // AVX-512 パスは未検証のため、既定では選択しない。
        // 明示的に有効化したい場合は環境変数 OPEN_CPU_ENABLE_AVX512=1 を設定する。
        if c.avx512f && c.avx512bw && avx512_opt_in() {
            return GfImpl::Avx512;
        }
        if c.avx2 {
            return GfImpl::Avx2;
        }
        if c.pclmulqdq && c.ssse3 {
            return GfImpl::Pclmul;
        }
    }
    let _ = c;
    GfImpl::Scalar
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn avx512_opt_in() -> bool {
    use std::sync::OnceLock;
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| std::env::var("OPEN_CPU_ENABLE_AVX512").as_deref() == Ok("1"))
}

/// `dst[i] ^= src[i]`(RAID6 の P パリティ蓄積)。
///
/// # Panics
/// `dst.len() != src.len()` の場合。
pub fn gf_xor(dst: &mut [u8], src: &[u8]) {
    assert_eq!(dst.len(), src.len(), "gf_xor: 長さが一致しません");
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        // AVX2 が使えるなら 32 byte/iter で処理する(端数はスカラーへ)。
        if detect().avx2 {
            let n = dst.len();
            let done = unsafe { xor_avx2(dst, src) };
            if done < n {
                gf_xor_scalar(&mut dst[done..], &src[done..]);
            }
            return;
        }
    }
    gf_xor_scalar(dst, src);
}

/// AVX2 による XOR(処理済みバイト数を返す)。
///
/// # Safety
/// 呼び出し元は AVX2 が利用可能であることを保証すること。
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn xor_avx2(dst: &mut [u8], src: &[u8]) -> usize {
    use x86::*;
    unsafe {
        let n = dst.len();
        let blocks = n / 32;
        let dp = dst.as_mut_ptr();
        let sp = src.as_ptr();
        for i in 0..blocks {
            let off = (i * 32) as isize;
            let a = _mm256_loadu_si256(dp.offset(off) as *const __m256i);
            let b = _mm256_loadu_si256(sp.offset(off) as *const __m256i);
            _mm256_storeu_si256(dp.offset(off) as *mut __m256i, _mm256_xor_si256(a, b));
        }
        blocks * 32
    }
}

// ---------------------------------------------------------------------------
// ホーナー法(acc = acc * 2^times ^ src)
// ---------------------------------------------------------------------------

/// GF(2^8) 上でバイトを 2 倍する(左 1bit シフト + 桁あふれ時に 0x1d を XOR)。
#[inline]
pub const fn gf_mul2_byte(b: u8) -> u8 {
    (b << 1) ^ (((b >> 7) & 1) * 0x1d)
}

const MASK_HIGH_U64: u64 = 0x8080_8080_8080_8080;
const MASK_LOW7_U64: u64 = 0x7f7f_7f7f_7f7f_7f7f;
const POLY_U64: u64 = 0x1d1d_1d1d_1d1d_1d1d;

/// u64 語に詰めた 8 バイトをそれぞれ GF(2^8) 上で 2 倍する。
#[inline(always)]
const fn mul2_u64(w: u64) -> u64 {
    let high = w & MASK_HIGH_U64;
    // 最上位ビットが立っているバイトを 0xff へ展開する
    let mask = (high >> 7) * 0xff;
    ((w & MASK_LOW7_U64) << 1) ^ (mask & POLY_U64)
}

/// `acc = acc * 2^times ^ src`(シンドローム畳み込みのホーナー法 1 ステップ)。
///
/// RAID6 の Q シンドローム `Q = Σ D_i · 2^i` は係数テーブルを持たずに
/// `q = 0; for d in disks.rev() { q = q*2 ^ d }` で計算できる。`times = 2` に
/// すれば RAID-Z3 の R シンドローム(`Σ D_i · 4^i`)にも使える。
///
/// `times = 0` は単なる XOR([`gf_xor`])と等価。
///
/// # Panics
/// `acc.len() != src.len()` の場合。
pub fn gf_mul_pow2_xor(acc: &mut [u8], src: &[u8], times: u32) {
    assert_eq!(acc.len(), src.len(), "gf_mul_pow2_xor: 長さが一致しません");
    if times == 0 {
        gf_xor(acc, src);
        return;
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if detect().avx2 {
            let n = acc.len();
            let done = unsafe { mul_pow2_xor_avx2(acc, src, times) };
            if done < n {
                gf_mul_pow2_xor_scalar(&mut acc[done..], &src[done..], times);
            }
            return;
        }
    }
    gf_mul_pow2_xor_scalar(acc, src, times);
}

/// `acc = acc * 2 ^ src`([`gf_mul_pow2_xor`] の `times = 1`)。
pub fn gf_mul2_xor(acc: &mut [u8], src: &[u8]) {
    gf_mul_pow2_xor(acc, src, 1);
}

/// `acc = acc * 4 ^ src`([`gf_mul_pow2_xor`] の `times = 2`)。
pub fn gf_mul4_xor(acc: &mut [u8], src: &[u8]) {
    gf_mul_pow2_xor(acc, src, 2);
}

/// ホーナー法のスカラー実装(u64 ビットトリック)。全アーキテクチャ共通。
pub fn gf_mul_pow2_xor_scalar(acc: &mut [u8], src: &[u8], times: u32) {
    assert_eq!(acc.len(), src.len());
    let n = acc.len();
    let chunks = n / 8;
    for i in 0..chunks {
        let o = i * 8;
        let mut w = u64::from_ne_bytes(acc[o..o + 8].try_into().unwrap());
        for _ in 0..times {
            w = mul2_u64(w);
        }
        let s = u64::from_ne_bytes(src[o..o + 8].try_into().unwrap());
        acc[o..o + 8].copy_from_slice(&(w ^ s).to_ne_bytes());
    }
    for i in chunks * 8..n {
        let mut b = acc[i];
        for _ in 0..times {
            b = gf_mul2_byte(b);
        }
        acc[i] = b ^ src[i];
    }
}

/// ホーナー法の AVX2 実装(32 byte/iter、処理済みバイト数を返す)。
///
/// # Safety
/// 呼び出し元は AVX2 が利用可能であることを保証すること。
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
unsafe fn mul_pow2_xor_avx2(acc: &mut [u8], src: &[u8], times: u32) -> usize {
    use x86::*;
    unsafe {
        let n = acc.len();
        let blocks = n / 32;
        let ap = acc.as_mut_ptr();
        let sp = src.as_ptr();
        let zero = _mm256_setzero_si256();
        let poly = _mm256_set1_epi8(0x1du8 as i8);
        let low7 = _mm256_set1_epi8(0x7fu8 as i8);
        for i in 0..blocks {
            let off = (i * 32) as isize;
            let mut a = _mm256_loadu_si256(ap.offset(off) as *const __m256i);
            for _ in 0..times {
                // 符号付き比較 0 > a は「最上位ビットが立っているバイト」で真
                let mask = _mm256_cmpgt_epi8(zero, a);
                let shifted = _mm256_slli_epi64(_mm256_and_si256(a, low7), 1);
                a = _mm256_xor_si256(shifted, _mm256_and_si256(mask, poly));
            }
            let s = _mm256_loadu_si256(sp.offset(off) as *const __m256i);
            _mm256_storeu_si256(ap.offset(off) as *mut __m256i, _mm256_xor_si256(a, s));
        }
        blocks * 32
    }
}

/// スカラー(u64 語単位)の XOR。全アーキテクチャで動作する参照実装。
pub fn gf_xor_scalar(dst: &mut [u8], src: &[u8]) {
    assert_eq!(dst.len(), src.len());
    let n = dst.len();
    let chunks = n / 8;
    for i in 0..chunks {
        let o = i * 8;
        let a = u64::from_ne_bytes(dst[o..o + 8].try_into().unwrap());
        let b = u64::from_ne_bytes(src[o..o + 8].try_into().unwrap());
        dst[o..o + 8].copy_from_slice(&(a ^ b).to_ne_bytes());
    }
    for i in chunks * 8..n {
        dst[i] ^= src[i];
    }
}

/// `dst[i] ^= gf_mul(src[i], factor)`(RAID6 の Q パリティ蓄積)。
///
/// RAID6 の Q パリティは `Q = Σ g^i · D_i` であり、各データストライプに
/// 係数 `g^i` を掛けて XOR 蓄積する形で計算できる。本関数はその 1 ストライプ分
/// を担う汎用 API で、`open-raid-z` 等から呼び出すことを想定している。
///
/// 実装は実行時に CPU 機能へディスパッチされる([`selected_impl`])。
///
/// # Panics
/// `dst.len() != src.len()` の場合。
pub fn gf_mul_parity(dst: &mut [u8], src: &[u8], factor: u8) {
    assert_eq!(dst.len(), src.len(), "gf_mul_parity: 長さが一致しません");
    if factor == 0 {
        return; // 0 倍は無変化
    }
    if factor == 1 {
        gf_xor(dst, src);
        return;
    }
    match selected_impl() {
        GfImpl::Scalar => gf_mul_parity_scalar(dst, src, factor),
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        GfImpl::Pclmul => unsafe { gf_mul_parity_pclmul(dst, src, factor) },
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        GfImpl::Avx2 => unsafe { gf_mul_parity_avx2(dst, src, factor) },
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        GfImpl::Avx512 => unsafe { gf_mul_parity_avx512(dst, src, factor) },
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        _ => gf_mul_parity_scalar(dst, src, factor),
    }
}

/// スカラー(nibble split テーブル)実装。全アーキテクチャで動作する参照実装。
pub fn gf_mul_parity_scalar(dst: &mut [u8], src: &[u8], factor: u8) {
    assert_eq!(dst.len(), src.len());
    let t = split_table(factor);
    for (d, &s) in dst.iter_mut().zip(src.iter()) {
        *d ^= t.lo[(s & 0x0f) as usize] ^ t.hi[(s >> 4) as usize];
    }
}

// ---------------------------------------------------------------------------
// x86 SIMD 実装
// ---------------------------------------------------------------------------

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86 {
    #[cfg(target_arch = "x86")]
    pub use std::arch::x86::*;
    #[cfg(target_arch = "x86_64")]
    pub use std::arch::x86_64::*;
}

/// AVX2 実装(`vpshufb` split-table、32 byte/iter)。
///
/// # Safety
/// 呼び出し元は AVX2 が利用可能であることを保証すること。
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
pub unsafe fn gf_mul_parity_avx2(dst: &mut [u8], src: &[u8], factor: u8) {
    use x86::*;
    assert_eq!(dst.len(), src.len());
    unsafe {
        let t = split_table(factor);
        let tl = _mm256_broadcastsi128_si256(_mm_loadu_si128(t.lo.as_ptr() as *const __m128i));
        let th = _mm256_broadcastsi128_si256(_mm_loadu_si128(t.hi.as_ptr() as *const __m128i));
        let mask = _mm256_set1_epi8(0x0f);

        let n = dst.len();
        let blocks = n / 32;
        let dp = dst.as_mut_ptr();
        let sp = src.as_ptr();
        for i in 0..blocks {
            let off = (i * 32) as isize;
            let s = _mm256_loadu_si256(sp.offset(off) as *const __m256i);
            let lo = _mm256_and_si256(s, mask);
            let hi = _mm256_and_si256(_mm256_srli_epi64(s, 4), mask);
            let prod = _mm256_xor_si256(_mm256_shuffle_epi8(tl, lo), _mm256_shuffle_epi8(th, hi));
            let d = _mm256_loadu_si256(dp.offset(off) as *const __m256i);
            _mm256_storeu_si256(dp.offset(off) as *mut __m256i, _mm256_xor_si256(d, prod));
        }
        let done = blocks * 32;
        if done < n {
            gf_mul_parity_scalar(&mut dst[done..], &src[done..], factor);
        }
    }
}

/// AVX-512F/BW 実装(`vpshufb` split-table、64 byte/iter)。
///
/// **未検証**: 開発機(AMD Ryzen 9 3950X)は AVX-512 非搭載のため、
/// コンパイルが通ることのみを確認しており、実行検証はしていない。
/// 既定のディスパッチでは選択されない(`OPEN_CPU_ENABLE_AVX512=1` で opt-in)。
///
/// # Safety
/// 呼び出し元は AVX-512F/BW が利用可能であることを保証すること。
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx512f,avx512bw")]
pub unsafe fn gf_mul_parity_avx512(dst: &mut [u8], src: &[u8], factor: u8) {
    use x86::*;
    assert_eq!(dst.len(), src.len());
    unsafe {
        let t = split_table(factor);
        let tl = _mm512_broadcast_i32x4(_mm_loadu_si128(t.lo.as_ptr() as *const __m128i));
        let th = _mm512_broadcast_i32x4(_mm_loadu_si128(t.hi.as_ptr() as *const __m128i));
        let mask = _mm512_set1_epi8(0x0f);

        let n = dst.len();
        let blocks = n / 64;
        let dp = dst.as_mut_ptr();
        let sp = src.as_ptr();
        for i in 0..blocks {
            let off = (i * 64) as isize;
            let s = _mm512_loadu_si512(sp.offset(off) as *const _);
            let lo = _mm512_and_si512(s, mask);
            let hi = _mm512_and_si512(_mm512_srli_epi64(s, 4), mask);
            let prod = _mm512_xor_si512(_mm512_shuffle_epi8(tl, lo), _mm512_shuffle_epi8(th, hi));
            let d = _mm512_loadu_si512(dp.offset(off) as *const _);
            _mm512_storeu_si512(dp.offset(off) as *mut _, _mm512_xor_si512(d, prod));
        }
        let done = blocks * 64;
        if done < n {
            // 端数は AVX2 相当(呼び出し元が AVX-512 対応なら AVX2 も必ず対応)
            gf_mul_parity_avx2(&mut dst[done..], &src[done..], factor);
        }
    }
}

/// PCLMULQDQ 実装(キャリーレス乗算 + `pshufb` 還元、16 byte/iter)。
///
/// 各バイトを 16bit 間隔に展開すると、8bit 係数とのキャリーレス積(最大 15bit)
/// が隣接スロットへ桁上がりしないため、64bit の `pclmulqdq` 1 命令で
/// 4 バイト分をまとめて乗算できる。その後 16bit 値を 2 回の `pshufb` で
/// GF(2^8) へ還元する。
///
/// # Safety
/// 呼び出し元は SSSE3 と PCLMULQDQ が利用可能であることを保証すること。
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "sse2,ssse3,pclmulqdq")]
pub unsafe fn gf_mul_parity_pclmul(dst: &mut [u8], src: &[u8], factor: u8) {
    use x86::*;
    assert_eq!(dst.len(), src.len());

    unsafe {
        let f = _mm_set_epi64x(0, factor as i64);
        let red_lo = _mm_loadu_si128(RED_LO.as_ptr() as *const __m128i);
        let red_hi = _mm_loadu_si128(RED_HI.as_ptr() as *const __m128i);
        let zero = _mm_setzero_si128();
        let m_lowbyte = _mm_set1_epi16(0x00ff);
        let m_nib = _mm_set1_epi16(0x000f);
        let m_msb = _mm_set1_epi16(0x8000u16 as i16);

        // 8 個の u16 レーン(= 8 バイト分)を係数倍して還元する。
        #[inline]
        #[target_feature(enable = "sse2,ssse3,pclmulqdq")]
        unsafe fn mul_reduce(
            v: __m128i,
            f: __m128i,
            red_lo: __m128i,
            red_hi: __m128i,
            m_lowbyte: __m128i,
            m_nib: __m128i,
            m_msb: __m128i,
        ) -> __m128i {
            let a = _mm_clmulepi64_si128(v, f, 0x00);
            let b = _mm_clmulepi64_si128(v, f, 0x01);
            let p = _mm_unpacklo_epi64(a, b); // 8 レーン × 最大 15bit
            let lb = _mm_and_si128(p, m_lowbyte);
            let hb = _mm_srli_epi16(p, 8); // 0..=0x7f
            let nl = _mm_or_si128(_mm_and_si128(hb, m_nib), m_msb);
            let nh = _mm_or_si128(_mm_srli_epi16(hb, 4), m_msb);
            let r = _mm_xor_si128(_mm_shuffle_epi8(red_lo, nl), _mm_shuffle_epi8(red_hi, nh));
            _mm_xor_si128(lb, r)
        }

        let n = dst.len();
        let blocks = n / 16;
        let dp = dst.as_mut_ptr();
        let sp = src.as_ptr();
        for i in 0..blocks {
            let off = (i * 16) as isize;
            let s = _mm_loadu_si128(sp.offset(off) as *const __m128i);
            let lo = _mm_unpacklo_epi8(s, zero);
            let hi = _mm_unpackhi_epi8(s, zero);
            let rl = mul_reduce(lo, f, red_lo, red_hi, m_lowbyte, m_nib, m_msb);
            let rh = mul_reduce(hi, f, red_lo, red_hi, m_lowbyte, m_nib, m_msb);
            let prod = _mm_packus_epi16(rl, rh);
            let d = _mm_loadu_si128(dp.offset(off) as *const __m128i);
            _mm_storeu_si128(dp.offset(off) as *mut __m128i, _mm_xor_si128(d, prod));
        }
        let done = blocks * 16;
        if done < n {
            gf_mul_parity_scalar(&mut dst[done..], &src[done..], factor);
        }
    }
}

// ---------------------------------------------------------------------------
// RAID6 パリティのユーティリティ
// ---------------------------------------------------------------------------

/// RAID6 の生成元 `g = 2`(GF(2^8) の原始元)。
pub const RAID6_GENERATOR: u8 = 2;

/// `g^i` を求める(RAID6 の Q パリティ係数)。
pub fn raid6_coeff(i: usize) -> u8 {
    let mut r: u8 = 1;
    for _ in 0..i {
        r = gf_mul(r, RAID6_GENERATOR);
    }
    r
}

/// データストライプ列から RAID6 の P/Q パリティを計算する。
///
/// すべてのストライプは同じ長さでなければならない。
///
/// # Panics
/// 長さが不揃いの場合。
pub fn raid6_parity(stripes: &[&[u8]], p: &mut [u8], q: &mut [u8]) {
    assert_eq!(p.len(), q.len(), "raid6_parity: P/Q の長さが不一致");
    p.fill(0);
    q.fill(0);
    for (i, s) in stripes.iter().enumerate() {
        assert_eq!(s.len(), p.len(), "raid6_parity: ストライプ長が不一致");
        gf_xor(p, s);
        gf_mul_parity(q, s, raid6_coeff(i));
    }
}

/// RAID-Z3 相当の P/Q/R シンドロームを、ホーナー法で一括計算する。
///
/// - `P = Σ D_i`(単純 XOR)
/// - `Q = Σ D_i · 2^i`
/// - `R = Σ D_i · 4^i`
///
/// 係数テーブルを引かずに済むため [`raid6_parity`] より高速。
///
/// # Panics
/// 長さが不揃いの場合。
pub fn raid6_parity3(stripes: &[&[u8]], p: &mut [u8], q: &mut [u8], r: &mut [u8]) {
    assert_eq!(p.len(), q.len(), "raid6_parity3: P/Q の長さが不一致");
    assert_eq!(p.len(), r.len(), "raid6_parity3: P/R の長さが不一致");
    p.fill(0);
    q.fill(0);
    r.fill(0);
    for s in stripes.iter() {
        assert_eq!(s.len(), p.len(), "raid6_parity3: ストライプ長が不一致");
        gf_xor(p, s);
    }
    // ホーナー法は末尾のディスクから畳み込む
    for s in stripes.iter().rev() {
        gf_mul2_xor(q, s);
        gf_mul4_xor(r, s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(n: usize) -> Vec<u8> {
        // 決定的な擬似乱数(xorshift)
        let mut x: u32 = 0x1234_5678;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x & 0xff) as u8
            })
            .collect()
    }

    /// 素朴なビットシフト実装を基準に、テーブル版スカラーの正しさを確認。
    #[test]
    fn scalar_matches_naive() {
        let src = sample(257);
        for factor in 0u16..=255 {
            let factor = factor as u8;
            let mut a = vec![0x5au8; src.len()];
            let mut b = a.clone();
            gf_mul_parity_scalar(&mut a, &src, factor);
            for (d, &s) in b.iter_mut().zip(src.iter()) {
                *d ^= gf_mul(s, factor);
            }
            assert_eq!(a, b, "factor={factor}");
        }
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[test]
    fn avx2_matches_scalar() {
        if !detect().avx2 {
            eprintln!("AVX2 非対応の CPU のためスキップ");
            return;
        }
        // 端数処理も含めて検証するため 32 の倍数でない長さを混ぜる
        for len in [0usize, 1, 15, 31, 32, 33, 64, 100, 1000, 4096, 4097] {
            let src = sample(len);
            for factor in [0u8, 1, 2, 3, 17, 0x80, 0xff, 0x1d] {
                let mut a = sample(len);
                let mut b = a.clone();
                gf_mul_parity_scalar(&mut a, &src, factor);
                unsafe { gf_mul_parity_avx2(&mut b, &src, factor) };
                assert_eq!(a, b, "len={len} factor={factor}");
            }
        }
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[test]
    fn pclmul_matches_scalar() {
        let c = detect();
        if !(c.pclmulqdq && c.ssse3) {
            eprintln!("PCLMULQDQ/SSSE3 非対応の CPU のためスキップ");
            return;
        }
        for len in [0usize, 1, 15, 16, 17, 31, 64, 255, 4096] {
            let src = sample(len);
            for factor in 0u16..=255 {
                let factor = factor as u8;
                let mut a = sample(len);
                let mut b = a.clone();
                gf_mul_parity_scalar(&mut a, &src, factor);
                unsafe { gf_mul_parity_pclmul(&mut b, &src, factor) };
                assert_eq!(a, b, "len={len} factor={factor}");
            }
        }
    }

    /// AVX-512 は開発機に無いため、ここでは「選択されないこと」だけを確認する。
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[test]
    fn avx512_not_selected_without_support() {
        if !detect().avx512f {
            assert_ne!(selected_impl(), GfImpl::Avx512);
        }
    }

    #[test]
    fn dispatch_matches_scalar() {
        let src = sample(3333);
        for factor in [0u8, 1, 7, 0x1d, 0xff] {
            let mut a = sample(3333);
            let mut b = a.clone();
            gf_mul_parity_scalar(&mut a, &src, factor);
            gf_mul_parity(&mut b, &src, factor);
            assert_eq!(a, b, "factor={factor}");
        }
    }

    #[test]
    fn mul2_byte_matches_gf_mul() {
        for b in 0..=255u8 {
            assert_eq!(gf_mul2_byte(b), gf_mul(b, 2), "b={b:#04x}");
        }
    }

    #[test]
    fn mul_pow2_xor_matches_naive() {
        for len in [0usize, 1, 7, 8, 31, 32, 33, 64, 1000, 4096, 4097] {
            let src = sample(len);
            for times in 0u32..=3 {
                let start = sample(len);
                let mut a = start.clone();
                gf_mul_pow2_xor(&mut a, &src, times);

                let mut b = start.clone();
                gf_mul_pow2_xor_scalar(&mut b, &src, times);
                assert_eq!(a, b, "dispatch vs scalar len={len} times={times}");

                // 素朴なバイト単位計算を基準に検証
                let expect: Vec<u8> = start
                    .iter()
                    .zip(src.iter())
                    .map(|(&x, &s)| {
                        let mut v = x;
                        for _ in 0..times {
                            v = gf_mul(v, 2);
                        }
                        v ^ s
                    })
                    .collect();
                assert_eq!(a, expect, "vs naive len={len} times={times}");
            }
        }
    }

    /// ホーナー法で畳み込んだ Q シンドロームが、係数テーブル方式
    /// (`raid6_parity`)の結果と完全一致することを確認する。
    #[test]
    fn horner_q_matches_coefficient_form() {
        let len = 777;
        let disks: Vec<Vec<u8>> = (0..6)
            .map(|k| sample(len).iter().map(|x| x.wrapping_add(k)).collect())
            .collect();
        let refs: Vec<&[u8]> = disks.iter().map(|d| d.as_slice()).collect();

        let mut p = vec![0u8; len];
        let mut q = vec![0u8; len];
        raid6_parity(&refs, &mut p, &mut q);

        // ホーナー法: q = 0; for d in disks.rev() { q = q*2 ^ d }
        let mut hq = vec![0u8; len];
        for d in refs.iter().rev() {
            gf_mul2_xor(&mut hq, d);
        }
        assert_eq!(hq, q, "ホーナー法と係数テーブル方式のQが不一致");
    }

    #[test]
    fn raid6_parity3_matches_naive() {
        let len = 500;
        let disks: Vec<Vec<u8>> = (0..5)
            .map(|k| {
                sample(len)
                    .iter()
                    .map(|x| x.wrapping_mul(3).wrapping_add(k))
                    .collect()
            })
            .collect();
        let refs: Vec<&[u8]> = disks.iter().map(|d| d.as_slice()).collect();

        let mut p = vec![0u8; len];
        let mut q = vec![0u8; len];
        let mut r = vec![0u8; len];
        raid6_parity3(&refs, &mut p, &mut q, &mut r);

        // 素朴な係数形との突き合わせ: P=Σd, Q=Σ d·2^i, R=Σ d·4^i
        let mut np = vec![0u8; len];
        let mut nq = vec![0u8; len];
        let mut nr = vec![0u8; len];
        for (i, d) in refs.iter().enumerate() {
            let mut c2: u8 = 1;
            let mut c4: u8 = 1;
            for _ in 0..i {
                c2 = gf_mul(c2, 2);
                c4 = gf_mul(c4, 4);
            }
            for j in 0..len {
                np[j] ^= d[j];
                nq[j] ^= gf_mul(d[j], c2);
                nr[j] ^= gf_mul(d[j], c4);
            }
        }
        assert_eq!(p, np, "P");
        assert_eq!(q, nq, "Q");
        assert_eq!(r, nr, "R");
    }

    #[test]
    fn xor_works() {
        for len in [0usize, 1, 7, 31, 32, 33, 1001, 4096] {
            let src = sample(len);
            let mut a = sample(len);
            let expect: Vec<u8> = a.iter().zip(src.iter()).map(|(x, y)| x ^ y).collect();
            gf_xor(&mut a, &src);
            assert_eq!(a, expect, "len={len}");
            let mut b = sample(len);
            gf_xor_scalar(&mut b, &src);
            assert_eq!(b, expect, "scalar len={len}");
        }
        let src = sample(1001);
        let mut a = sample(1001);
        let expect: Vec<u8> = a.iter().zip(src.iter()).map(|(x, y)| x ^ y).collect();
        gf_xor(&mut a, &src);
        assert_eq!(a, expect);
    }

    #[test]
    fn gf_field_axioms() {
        assert_eq!(gf_mul(1, 0x57), 0x57);
        assert_eq!(gf_mul(0, 0x57), 0);
        // 交換則
        for a in [1u8, 2, 3, 0x1d, 0xff] {
            for b in [1u8, 5, 0x80, 0xfe] {
                assert_eq!(gf_mul(a, b), gf_mul(b, a));
            }
        }
        // 分配則
        for a in 0u16..=255 {
            let a = a as u8;
            assert_eq!(gf_mul(a, 3), gf_mul(a, 1) ^ gf_mul(a, 2));
        }
    }

    #[test]
    fn raid6_generator_is_primitive() {
        // g=2 の位数は 255(原始元)であること
        let mut x: u8 = 1;
        for i in 1..255 {
            x = gf_mul(x, RAID6_GENERATOR);
            assert_ne!(x, 1, "位数が {i} で 1 に戻った");
        }
        assert_eq!(gf_mul(x, RAID6_GENERATOR), 1);
    }

    #[test]
    fn raid6_parity_recovers_single_failure() {
        let len = 512;
        let d0 = sample(len);
        let d1: Vec<u8> = sample(len).iter().map(|x| x.wrapping_add(9)).collect();
        let d2: Vec<u8> = sample(len).iter().map(|x| !x).collect();
        let mut p = vec![0u8; len];
        let mut q = vec![0u8; len];
        raid6_parity(&[&d0, &d1, &d2], &mut p, &mut q);

        // d1 を失ったと仮定して P から復元
        let mut rec = p.clone();
        gf_xor(&mut rec, &d0);
        gf_xor(&mut rec, &d2);
        assert_eq!(rec, d1);

        // Q からも復元できること: Q ^ g^0*d0 ^ g^2*d2 = g^1 * d1
        let mut t = q.clone();
        gf_mul_parity(&mut t, &d0, raid6_coeff(0));
        gf_mul_parity(&mut t, &d2, raid6_coeff(2));
        let expect: Vec<u8> = d1.iter().map(|&x| gf_mul(x, raid6_coeff(1))).collect();
        assert_eq!(t, expect);
    }
}
