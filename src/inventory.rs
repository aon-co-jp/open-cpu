//! CPU 命令セットの **全自動インベントリ**(2026-09-21新設、ユーザー指示「スマホのCPUもopen-cpuで
//! AVX2やAVX512を始め、ハードウェア・アクセラレーター対応のCPUの命令を全て自動確認する機能」)。
//!
//! - x86/x86_64: SSE〜AVX-512 各サブセット・AVX-VNNI・AMX相当・GFNI・VAES・BMI・SHA・AES 等を
//!   `is_x86_feature_detected!` で確認。
//! - aarch64(スマホ/タブレット/Apple Silicon 等): NEON(asimd)・FP16・dotprod・i8mm・bf16・SVE/SVE2・
//!   AES/PMULL/SHA2/SHA3・CRC32・LSE(atomics)・RCpc 等を `is_aarch64_feature_detected!` で確認。
//! - Linux/Android: `/proc/cpuinfo` の生フラグ(`Features`/`flags`)と、aarch64 の CPU part から
//!   コア構成(例: Cortex-A78×2 + Cortex-A55×6)も取得する(big.LITTLE の把握用)。
//!
//! **正直な開示**: これは「そのCPUが持っている命令」の検出であり、open-cpu の演算カーネルが
//! その命令を実際に使うかどうかとは別(`used_by_open_cpu` で区別する)。ARM 向けの専用カーネルは
//! まだ無く、ARM では検出のみ(コンパイラの自動ベクトル化はNEONを使い得る)。
//! `/proc/cpuinfo` が読めない環境ではその項目が空になる(検出できなかったことを 0 台と偽らない)。

use std::sync::OnceLock;

/// 1 つの機能の検出結果。
#[derive(Debug, Clone)]
pub struct FeatureStatus {
    pub name: &'static str,
    pub detected: bool,
    /// open-cpu のいずれかの演算カーネルが実際にこの機能を使う実装を持っているか。
    pub used_by_open_cpu: bool,
}

/// aarch64 のコア種別ごとの数(big.LITTLE 把握用)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreGroup {
    pub name: String,
    pub count: usize,
}

/// CPU 全体のインベントリ。
#[derive(Debug, Clone)]
pub struct CpuInventory {
    pub arch: &'static str,
    pub features: Vec<FeatureStatus>,
    /// `/proc/cpuinfo` の生フラグ(取得できた場合のみ)。
    pub raw_flags: Vec<String>,
    /// aarch64 Linux/Android のコア構成(取得できた場合のみ)。
    pub cores: Vec<CoreGroup>,
    /// 検出経路の説明("rustc-macro" / "rustc-macro+procfs" 等)。
    pub source: &'static str,
}

impl CpuInventory {
    /// 検出できた機能名(スペース区切りではなく Vec)。
    pub fn detected_names(&self) -> Vec<&'static str> {
        self.features.iter().filter(|f| f.detected).map(|f| f.name).collect()
    }

    /// 人間可読の複数行サマリ。
    pub fn summary(&self) -> String {
        let mut s = format!("arch: {} (source: {})\n", self.arch, self.source);
        s.push_str(&format!("detected: {}\n", self.detected_names().join(" ")));
        let missing: Vec<&str> = self.features.iter().filter(|f| !f.detected).map(|f| f.name).collect();
        s.push_str(&format!("not detected: {}\n", missing.join(" ")));
        if !self.cores.is_empty() {
            let c: Vec<String> = self.cores.iter().map(|g| format!("{} x{}", g.name, g.count)).collect();
            s.push_str(&format!("cores: {}\n", c.join(", ")));
        }
        s
    }

    /// 依存クレート無しの簡易JSON(aruaru-llm等がそのまま埋め込める)。
    pub fn to_json(&self) -> String {
        fn esc(s: &str) -> String {
            s.replace('\\', "\\\\").replace('"', "\\\"")
        }
        let feats: Vec<String> = self
            .features
            .iter()
            .map(|f| format!("{{\"name\":\"{}\",\"detected\":{},\"used_by_open_cpu\":{}}}", esc(f.name), f.detected, f.used_by_open_cpu))
            .collect();
        let cores: Vec<String> = self.cores.iter().map(|g| format!("{{\"name\":\"{}\",\"count\":{}}}", esc(&g.name), g.count)).collect();
        let raw: Vec<String> = self.raw_flags.iter().map(|r| format!("\"{}\"", esc(r))).collect();
        format!(
            "{{\"arch\":\"{}\",\"source\":\"{}\",\"features\":[{}],\"cores\":[{}],\"raw_flags\":[{}]}}",
            self.arch,
            self.source,
            feats.join(","),
            cores.join(","),
            raw.join(",")
        )
    }
}

static INVENTORY: OnceLock<CpuInventory> = OnceLock::new();

/// CPU インベントリを取得する(初回のみ検出しキャッシュ)。
pub fn inventory() -> &'static CpuInventory {
    INVENTORY.get_or_init(build_inventory)
}

#[allow(unused_macros)]
macro_rules! feat_x86 {
    ($used:expr; $($name:tt),* $(,)?) => {
        vec![$(FeatureStatus { name: $name, detected: std::is_x86_feature_detected!($name), used_by_open_cpu: $used.contains(&$name) }),*]
    };
}

#[allow(unused_macros)]
macro_rules! feat_arm {
    ($($name:tt),* $(,)?) => {
        vec![$(FeatureStatus { name: $name, detected: std::arch::is_aarch64_feature_detected!($name), used_by_open_cpu: false }),*]
    };
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn detect_features() -> (&'static str, Vec<FeatureStatus>) {
    let used: Vec<&str> = crate::implemented_features().iter().map(|f| f.name()).collect();
    let used: &[&str] = &used;
    (
        if cfg!(target_arch = "x86_64") { "x86_64" } else { "x86" },
        feat_x86!(used;
            "sse", "sse2", "sse3", "ssse3", "sse4.1", "sse4.2", "avx", "avx2", "fma", "f16c",
            "bmi1", "bmi2", "popcnt", "lzcnt", "movbe", "adx", "aes", "pclmulqdq", "sha", "rdrand", "rdseed",
            "avx512f", "avx512cd", "avx512bw", "avx512dq", "avx512vl", "avx512ifma", "avx512vbmi",
            "avx512vbmi2", "avx512vnni", "avx512bitalg", "avx512vpopcntdq", "avx512bf16", "avx512fp16",
            "avxvnni", "gfni", "vaes", "vpclmulqdq",
        ),
    )
}

#[cfg(target_arch = "aarch64")]
fn detect_features() -> (&'static str, Vec<FeatureStatus>) {
    (
        "aarch64",
        feat_arm!(
            "neon", "fp16", "fhm", "fcma", "aes", "pmull", "sha2", "sha3", "sm4", "crc", "lse", "lse2", "rdm",
            "dotprod", "i8mm", "bf16", "f32mm", "f64mm", "sve", "sve2", "rcpc", "rcpc2", "dpb", "dpb2",
            "frintts", "jsconv", "flagm", "dit", "sb", "ssbs", "bti", "paca", "pacg", "rand", "mte",
        ),
    )
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
fn detect_features() -> (&'static str, Vec<FeatureStatus>) {
    (std::env::consts::ARCH, Vec::new())
}

/// aarch64 の `CPU part` → コア名(主要なものだけ。未知は 0x.. のまま返す)。
pub(crate) fn arm_core_name(implementer: u32, part: u32) -> String {
    if implementer == 0x51 {
        // Qualcomm Kryo(ARMコアの派生。公式ブランド名とベースコアの対応は公開情報に基づく目安)
        let q = match part {
            0x800 => "Kryo 2xx Gold (Cortex-A73 based)",
            0x801 => "Kryo 2xx Silver (Cortex-A53 based)",
            0x802 => "Kryo 3xx Gold (Cortex-A75 based)",
            0x803 => "Kryo 3xx Silver (Cortex-A55 based)",
            0x804 => "Kryo 4xx Gold (Cortex-A76 based)",
            0x805 => "Kryo 4xx Silver (Cortex-A55 based)",
            _ => "",
        };
        if !q.is_empty() {
            return q.to_string();
        }
    }
    match part {
        0xd03 => "Cortex-A53", 0xd04 => "Cortex-A35", 0xd05 => "Cortex-A55", 0xd07 => "Cortex-A57",
        0xd08 => "Cortex-A72", 0xd09 => "Cortex-A73", 0xd0a => "Cortex-A75", 0xd0b => "Cortex-A76",
        0xd0c => "Neoverse-N1", 0xd0d => "Cortex-A77", 0xd0e => "Cortex-A76AE", 0xd40 => "Neoverse-V1",
        0xd41 => "Cortex-A78", 0xd42 => "Cortex-A78AE", 0xd44 => "Cortex-X1", 0xd46 => "Cortex-A510",
        0xd47 => "Cortex-A710", 0xd48 => "Cortex-X2", 0xd49 => "Neoverse-N2", 0xd4a => "Neoverse-E1",
        0xd4b => "Cortex-A78C", 0xd4d => "Cortex-A715", 0xd4e => "Cortex-X3", 0xd80 => "Cortex-A520",
        0xd81 => "Cortex-A720", 0xd82 => "Cortex-X4", 0xd85 => "Cortex-X925", 0xd87 => "Cortex-A725",
        other => return format!("ARM implementer 0x{implementer:x} part 0x{other:x}"),
    }
    .to_string()
}

/// `/proc/cpuinfo` の中身から、生フラグ(x86 は `flags`、ARM は `Features`)とコア構成を取り出す。
pub(crate) fn parse_cpuinfo(text: &str) -> (Vec<String>, Vec<CoreGroup>) {
    let mut flags: Vec<String> = Vec::new();
    let mut parts: Vec<(u32, u32)> = Vec::new();
    let mut cur_impl: u32 = 0x41;
    for line in text.lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        let key = k.trim();
        let val = v.trim();
        if (key == "flags" || key == "Features") && flags.is_empty() {
            flags = val.split_whitespace().map(|s| s.to_string()).collect();
        } else if key == "CPU implementer" {
            if let Ok(i) = u32::from_str_radix(val.trim_start_matches("0x"), 16) {
                cur_impl = i;
            }
        } else if key == "CPU part" {
            if let Ok(p) = u32::from_str_radix(val.trim_start_matches("0x"), 16) {
                parts.push((cur_impl, p));
            }
        }
    }
    let mut cores: Vec<CoreGroup> = Vec::new();
    for (imp, p) in parts {
        let name = arm_core_name(imp, p);
        if let Some(g) = cores.iter_mut().find(|g| g.name == name) {
            g.count += 1;
        } else {
            cores.push(CoreGroup { name, count: 1 });
        }
    }
    (flags, cores)
}

fn build_inventory() -> CpuInventory {
    let (arch, features) = detect_features();
    let (raw_flags, cores) = match std::fs::read_to_string("/proc/cpuinfo") {
        Ok(t) => parse_cpuinfo(&t),
        Err(_) => (Vec::new(), Vec::new()),
    };
    let source = if raw_flags.is_empty() { "rustc-macro" } else { "rustc-macro+procfs" };
    CpuInventory { arch, features, raw_flags, cores, source }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_is_cached_and_nonempty_on_known_arch() {
        let a = inventory();
        let b = inventory();
        assert!(std::ptr::eq(a, b));
        if cfg!(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")) {
            assert!(!a.features.is_empty());
            assert!(a.to_json().starts_with("{\"arch\""));
        }
        println!("{}", a.summary());
    }

    #[test]
    fn parse_cpuinfo_arm_phone_sample() {
        // 実機(OPPO Reno11 A / MT6877)の /proc/cpuinfo 由来
        let sample = "CPU implementer	: 0x41
Features\t: fp asimd evtstrm aes pmull sha1 sha2 crc32 atomics fphp asimdhp cpuid asimdrdm lrcpc dcpop asimddp\nCPU part\t: 0xd05\nCPU part\t: 0xd05\nCPU part\t: 0xd41\n";
        let (flags, cores) = parse_cpuinfo(sample);
        assert!(flags.contains(&"asimddp".to_string()));
        assert_eq!(cores, vec![CoreGroup { name: "Cortex-A55".into(), count: 2 }, CoreGroup { name: "Cortex-A78".into(), count: 1 }]);
    }

    #[test]
    fn parse_cpuinfo_qualcomm_kryo_sample() {
        // 実機(moto g53y 5G / SM4350)由来: Kryo 4xx Gold x2 + Silver x6
        let mut sample = String::from("Features	: fp asimd aes asimddp
");
        for part in ["0x805", "0x805", "0x805", "0x805", "0x805", "0x805", "0x804", "0x804"] {
            sample.push_str(&format!("CPU implementer	: 0x51
CPU part	: {part}
"));
        }
        let (_, cores) = parse_cpuinfo(&sample);
        assert_eq!(cores.len(), 2);
        assert_eq!(cores[0].count, 6);
        assert!(cores[0].name.starts_with("Kryo 4xx Silver"));
        assert!(cores[1].name.starts_with("Kryo 4xx Gold"));
    }

    #[test]
    fn unknown_part_is_reported_not_hidden() {
        assert_eq!(arm_core_name(0x41, 0xfff), "ARM implementer 0x41 part 0xfff");
    }
}
