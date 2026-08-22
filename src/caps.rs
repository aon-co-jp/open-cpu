//! CPU 機能のランタイム検出。
//!
//! 検出は初回のみ実行し、結果を [`OnceLock`] にキャッシュする。
//! x86/x86_64 以外のアーキテクチャでは全フィールドが `false` になる
//! (スカラー実装へフォールバックする)。

use std::sync::OnceLock;

/// 検出された CPU 機能の一覧。
///
/// フィールドは「そのCPUが命令を持っているか」を表すだけで、
/// このクレートが実際にその命令を使った実装を持っているかどうかとは別。
/// 実装状況は README.md を参照。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuCapabilities {
    // --- 現状ディスパッチに使用しているもの ---
    pub avx2: bool,
    pub avx512f: bool,
    pub pclmulqdq: bool,
    // --- 検出のみ(将来利用) ---
    pub bmi1: bool,
    pub bmi2: bool,
    pub fma: bool,
    pub aes: bool,
    pub popcnt: bool,
    pub sha: bool,
    pub sse2: bool,
    pub ssse3: bool,
    pub avx512bw: bool,
    pub avx512vl: bool,
    /// AVX-VNNI (Alder Lake 以降)。フィールドと検出のみ、利用ロジックは未実装。
    pub avx_vnni: bool,
    /// AVX-512 VNNI。フィールドと検出のみ、利用ロジックは未実装。
    pub avx512vnni: bool,
}

static CAPS: OnceLock<CpuCapabilities> = OnceLock::new();

/// CPU 機能を検出する(初回のみ実際に検出し、以降はキャッシュを返す)。
pub fn detect() -> &'static CpuCapabilities {
    CAPS.get_or_init(detect_uncached)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn detect_uncached() -> CpuCapabilities {
    CpuCapabilities {
        avx2: std::is_x86_feature_detected!("avx2"),
        avx512f: std::is_x86_feature_detected!("avx512f"),
        pclmulqdq: std::is_x86_feature_detected!("pclmulqdq"),
        bmi1: std::is_x86_feature_detected!("bmi1"),
        bmi2: std::is_x86_feature_detected!("bmi2"),
        fma: std::is_x86_feature_detected!("fma"),
        aes: std::is_x86_feature_detected!("aes"),
        popcnt: std::is_x86_feature_detected!("popcnt"),
        sha: std::is_x86_feature_detected!("sha"),
        sse2: std::is_x86_feature_detected!("sse2"),
        ssse3: std::is_x86_feature_detected!("ssse3"),
        avx512bw: std::is_x86_feature_detected!("avx512bw"),
        avx512vl: std::is_x86_feature_detected!("avx512vl"),
        avx_vnni: std::is_x86_feature_detected!("avxvnni"),
        avx512vnni: std::is_x86_feature_detected!("avx512vnni"),
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
fn detect_uncached() -> CpuCapabilities {
    CpuCapabilities::default()
}

impl CpuCapabilities {
    /// 人間可読な一行サマリ(有効な機能名をスペース区切りで返す)。
    pub fn summary(&self) -> String {
        let mut v: Vec<&str> = Vec::new();
        for (name, on) in [
            ("sse2", self.sse2),
            ("ssse3", self.ssse3),
            ("popcnt", self.popcnt),
            ("aes", self.aes),
            ("pclmulqdq", self.pclmulqdq),
            ("bmi1", self.bmi1),
            ("bmi2", self.bmi2),
            ("fma", self.fma),
            ("sha", self.sha),
            ("avx2", self.avx2),
            ("avx512f", self.avx512f),
            ("avx512bw", self.avx512bw),
            ("avx512vl", self.avx512vl),
            ("avx-vnni", self.avx_vnni),
            ("avx512vnni", self.avx512vnni),
        ] {
            if on {
                v.push(name);
            }
        }
        if v.is_empty() {
            "(none)".to_string()
        } else {
            v.join(" ")
        }
    }
}
