//! # open-cpu
//!
//! `aon-co-jp` エコシステム共通の **CPU 命令セット検出・ランタイムディスパッチ**
//! ライブラリ。常駐サービスではなく、各リポジトリが `[dependencies]` で依存して
//! 同一プロセス内にリンクして使う通常の Rust ライブラリクレート。
//!
//! ## できること
//!
//! 1. [`detect`] で CPU 機能([`CpuCapabilities`])をランタイム検出(結果はキャッシュ)。
//! 2. RAID6 の GF(2^8) 演算([`gf_xor`] / [`gf_mul_parity`])を、
//!    検出結果に応じてスカラー / PCLMULQDQ / AVX2 / AVX-512 へ実行時ディスパッチ。
//!
//! ## 使い方
//!
//! ```
//! let caps = open_cpu::detect();
//! println!("CPU features: {}", caps.summary());
//!
//! // RAID6 Q パリティ: q ^= d * g^i
//! let d = vec![1u8, 2, 3, 4];
//! let mut q = vec![0u8; 4];
//! open_cpu::gf_mul_parity(&mut q, &d, open_cpu::raid6_coeff(1));
//! ```
//!
//! ## 注意(正直な実装状況)
//!
//! - AVX2 / PCLMULQDQ / スカラーの各実装は開発機(AMD Ryzen 9 3950X)で
//!   実行検証済み(スカラーとの出力一致をテストで確認)。
//! - **AVX-512 パスはコンパイル確認のみで実行未検証**(開発機が AVX-512 非搭載)。
//!   既定のディスパッチでは選択されず、`OPEN_CPU_ENABLE_AVX512=1` で opt-in する。
//! - BMI1/BMI2/FMA/AES/POPCNT/SHA/AVX-VNNI/AVX-512 VNNI は
//!   **検出フィールドのみ**で、これらを使う演算実装はまだ無い。
//! - [`gather_u8`]/[`gather_u8_avx2`](2026-09-13追加): `open-directx`の
//!   FFv1レンジコーダー実装(GPU側でNレーンが並列にテーブル値を
//!   lookupする設計、実GT730ハードウェアで32〜1536レーン検証済み)の
//!   CPU-SIMD側対応物として、AVX2の`vpgatherdd`によるバッチテーブル
//!   ルックアップを実装(この開発機・AMD Ryzen 9 3950Xで実行検証済み)。
//!   AVX-512版(16-wide gather)は、この開発機がAVX-512非搭載のため
//!   `gf.rs`のAVX-512パスと同じ理由で未実装。

#![forbid(unsafe_op_in_unsafe_fn)]

mod caps;
mod gather;
mod inventory;
mod gf;
mod isa;
mod math;

pub use caps::{detect, vendor_family, CpuCapabilities, CpuVendor};
pub use inventory::{inventory, CoreGroup, CpuInventory, FeatureStatus};
pub use isa::{
    avx512_opt_in, implemented_features, isa_summary, select, Feature, FeatureSet, IsaProfile,
    ALL_FEATURES, ALL_PROFILES,
};
pub use math::{
    axpy_f32, axpy_f32_scalar, bit_impl_summary, deposit_bits, deposit_bits_scalar, dot_f32,
    dot_f32_scalar, extract_bits, extract_bits_scalar, hamming_distance, hamming_distance_scalar,
    popcount_bytes, popcount_bytes_scalar, scale_f32, selected_float_impl, trailing_zeros_u64,
    FloatImpl,
};
pub use gf::{
    gf_mul, gf_mul2_byte, gf_mul2_xor, gf_mul4_xor, gf_mul_parity, gf_mul_parity_scalar,
    gf_mul_pow2_xor, gf_mul_pow2_xor_scalar, gf_xor, gf_xor_scalar, raid6_coeff, raid6_parity,
    raid6_parity3, selected_impl, GfImpl, GF_POLY, RAID6_GENERATOR,
};

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub use gf::{gf_mul_parity_avx2, gf_mul_parity_avx512, gf_mul_parity_pclmul};

pub use gather::{gather_u8, gather_u8_scalar};
#[cfg(target_arch = "x86_64")]
pub use gather::gather_u8_avx2;

/// クレートのバージョン文字列。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 検出結果と選択実装を 1 行にまとめた文字列(ログ出力用)。
pub fn runtime_summary() -> String {
    format!(
        "open-cpu {} | features: {} | gf impl: {:?}",
        VERSION,
        detect().summary(),
        selected_impl()
    )
}

/// 検出・組み合わせプロファイル・各カーネルの選択実装をまとめた複数行サマリ。
pub fn runtime_report() -> String {
    let caps = detect();
    format!(
        "open-cpu {}
  features: {}
  isa profile: {} (raw: {})
  gf impl: {:?}
  float impl: {}
  {}
  detected-but-unused: {}",
        VERSION,
        caps.feature_set(),
        caps.isa_profile(),
        caps.isa_profile_raw(),
        selected_impl(),
        selected_float_impl(),
        bit_impl_summary(),
        caps.detected_but_unused()
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn detect_is_cached_and_stable() {
        let a = super::detect();
        let b = super::detect();
        assert_eq!(a, b);
        assert!(std::ptr::eq(a, b), "OnceLock でキャッシュされていない");
    }

    #[test]
    fn summary_not_empty() {
        assert!(!super::runtime_summary().is_empty());
        println!("{}", super::runtime_summary());
    }
}
