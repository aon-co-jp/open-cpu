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
    /// GFNI(`vgf2p8mulb` / `vgf2p8affineqb`)。GF(2^8) 乗算を 1 命令で行える。
    ///
    /// **検出のみ**。Intel ISA-L 2.32 が split-table PSHUFB から GFNI へ
    /// 移行しており(Release_notes.txt: "Added new AVX2+GFNI and AVX512+GFNI
    /// pq_gen implementations")、このクレートでも RAID6 GF 演算の
    /// 高速パス候補になるが、開発機(Zen 2)が GFNI 非搭載のため未実装。
    pub gfni: bool,
    /// VPCLMULQDQ(256/512bit 幅のキャリーレス乗算)。**検出のみ**。
    pub vpclmulqdq: bool,
}

/// CPU ベンダ(BMI2 の速度特性を判定するために使う)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CpuVendor {
    Intel,
    Amd,
    #[default]
    Other,
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
        gfni: std::is_x86_feature_detected!("gfni"),
        vpclmulqdq: std::is_x86_feature_detected!("vpclmulqdq"),
    }
}

/// CPUID からベンダ文字列と family を読む(x86_64 のみ)。
#[cfg(target_arch = "x86_64")]
fn cpuid_vendor_family() -> (CpuVendor, u32) {
    use std::arch::x86_64::__cpuid;
    // leaf 0: ベンダ文字列 = EBX, EDX, ECX の順。
    let r0 = __cpuid(0);
    let mut name = [0u8; 12];
    name[0..4].copy_from_slice(&r0.ebx.to_le_bytes());
    name[4..8].copy_from_slice(&r0.edx.to_le_bytes());
    name[8..12].copy_from_slice(&r0.ecx.to_le_bytes());
    let vendor = match &name {
        b"GenuineIntel" => CpuVendor::Intel,
        b"AuthenticAMD" | b"HygonGenuine" => CpuVendor::Amd,
        _ => CpuVendor::Other,
    };
    // leaf 1: EAX[11:8] = base family、EAX[27:20] = extended family。
    let r1 = __cpuid(1);
    let base = (r1.eax >> 8) & 0xF;
    let family = if base == 0xF {
        base + ((r1.eax >> 20) & 0xFF)
    } else {
        base
    };
    (vendor, family)
}

static VENDOR_FAMILY: OnceLock<(CpuVendor, u32)> = OnceLock::new();

/// CPU ベンダと family(x86_64 以外では `(Other, 0)`)。
pub fn vendor_family() -> (CpuVendor, u32) {
    *VENDOR_FAMILY.get_or_init(|| {
        #[cfg(target_arch = "x86_64")]
        {
            cpuid_vendor_family()
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            (CpuVendor::Other, 0)
        }
    })
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
            ("gfni", self.gfni),
            ("vpclmulqdq", self.vpclmulqdq),
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

    /// PEXT / PDEP(BMI2)が **ハードウェア実装で速い** CPU かどうか。
    ///
    /// BMI2 の機能ビットが立っていても速いとは限らない。AMD の Zen /
    /// Zen+ / Zen 2(family 17h)および Hygon Dhyana では PEXT/PDEP が
    /// マイクロコード実装で非常に遅く、スカラーのループより遅いことが
    /// 知られている。AMD の最適化ガイド(family 19h = Zen 3)では
    /// 「ALU でネイティブ実行、スループット 1/cycle・レイテンシ 3 cycle。
    /// 高速/低速の経路を持つソフトウェアは family 19h では高速側を選ぶこと」
    /// と明記されており、Zen 3 以降で解消している。
    ///
    /// この開発機(Ryzen 9 3950X = Zen 2, family 17h)では **false** を返し、
    /// スカラー実装が選ばれる(実測でスカラーの方が速いことを確認済み)。
    pub fn fast_bmi2(&self) -> bool {
        if !self.bmi2 {
            return false;
        }
        let (vendor, family) = vendor_family();
        !(vendor == CpuVendor::Amd && family <= 0x17)
    }

    /// 指定した機能がすべて有効かどうか(呼び出し側の事前条件チェック用)。
    ///
    /// ```
    /// let caps = open_cpu::detect();
    /// if caps.has_all(&[caps.avx2, caps.fma]) {
    ///     // AVX2 と FMA3 の両方が使える経路
    /// }
    /// ```
    pub fn has_all(&self, features: &[bool]) -> bool {
        features.iter().all(|&f| f)
    }
}

impl std::fmt::Display for CpuCapabilities {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.summary())
    }
}
