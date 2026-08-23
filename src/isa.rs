//! 複数 CPU 命令セットの **組み合わせ** を扱うための API。
//!
//! [`crate::CpuCapabilities`] は「その命令を持っているか」を単独の bool で
//! 表すだけだったため、「AVX-512F と AVX-512BW が **両方** 揃っている場合のみ
//! このパスを使う」といった多軸の判定を呼び出し側が毎回手書きする必要があった。
//! このモジュールはその判定を型と関数として提供する。
//!
//! - [`Feature`]     : 命令セットを列挙型で表現(文字列名との相互変換つき)。
//! - [`FeatureSet`]  : 命令セットの集合(ビットマスク)。積集合・包含判定が可能。
//! - [`IsaProfile`]  : 実用上意味のある「組み合わせの段階」(スカラー〜AVX-512 VNNI)。
//! - [`select`]      : 候補パスを優先順に並べ、要求機能が全て揃う最初のものを選ぶ。
//!
//! 既存の [`crate::CpuCapabilities`] のフィールドは一切変更していないため、
//! 従来の `caps.avx2` のような書き方はそのまま動作する(後方互換)。

use crate::caps::{detect, CpuCapabilities};

/// 個別の CPU 命令セット。
///
/// 判別値がそのまま [`FeatureSet`] のビット位置になる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Feature {
    Sse2 = 0,
    Ssse3 = 1,
    Popcnt = 2,
    Aes = 3,
    Pclmulqdq = 4,
    Bmi1 = 5,
    Bmi2 = 6,
    Fma = 7,
    Sha = 8,
    Avx2 = 9,
    Avx512f = 10,
    Avx512bw = 11,
    Avx512vl = 12,
    AvxVnni = 13,
    Avx512vnni = 14,
    Gfni = 15,
    Vpclmulqdq = 16,
}

/// [`Feature`] の全列挙(順序は [`Feature`] の判別値順)。
pub const ALL_FEATURES: [Feature; 17] = [
    Feature::Sse2,
    Feature::Ssse3,
    Feature::Popcnt,
    Feature::Aes,
    Feature::Pclmulqdq,
    Feature::Bmi1,
    Feature::Bmi2,
    Feature::Fma,
    Feature::Sha,
    Feature::Avx2,
    Feature::Avx512f,
    Feature::Avx512bw,
    Feature::Avx512vl,
    Feature::AvxVnni,
    Feature::Avx512vnni,
    Feature::Gfni,
    Feature::Vpclmulqdq,
];

impl Feature {
    /// 正式名称(`is_x86_feature_detected!` に渡す文字列と同じ綴り)。
    pub const fn name(self) -> &'static str {
        match self {
            Feature::Sse2 => "sse2",
            Feature::Ssse3 => "ssse3",
            Feature::Popcnt => "popcnt",
            Feature::Aes => "aes",
            Feature::Pclmulqdq => "pclmulqdq",
            Feature::Bmi1 => "bmi1",
            Feature::Bmi2 => "bmi2",
            Feature::Fma => "fma",
            Feature::Sha => "sha",
            Feature::Avx2 => "avx2",
            Feature::Avx512f => "avx512f",
            Feature::Avx512bw => "avx512bw",
            Feature::Avx512vl => "avx512vl",
            Feature::AvxVnni => "avxvnni",
            Feature::Avx512vnni => "avx512vnni",
            Feature::Gfni => "gfni",
            Feature::Vpclmulqdq => "vpclmulqdq",
        }
    }

    /// 文字列から [`Feature`] を引く(`"avx-vnni"` のような表記ゆれも受ける)。
    pub fn from_name(s: &str) -> Option<Feature> {
        let norm = s.trim().to_ascii_lowercase().replace(['-', '_', '.'], "");
        ALL_FEATURES
            .iter()
            .copied()
            .find(|f| f.name().replace('-', "") == norm)
    }

    /// ビットマスク上の位置。
    pub const fn bit(self) -> u32 {
        1u32 << (self as u8)
    }
}

/// 命令セットの集合(17 bit のビットマスク)。
///
/// ```
/// use open_cpu::{Feature, FeatureSet};
/// let want = FeatureSet::from_slice(&[Feature::Avx2, Feature::Fma]);
/// let have = open_cpu::detect().feature_set();
/// if have.contains_all(want) {
///     // AVX2 と FMA3 の両方が揃っている場合のみのコードパス
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, PartialOrd, Ord, Hash)]
pub struct FeatureSet(u32);

impl FeatureSet {
    /// 空集合。
    pub const EMPTY: FeatureSet = FeatureSet(0);

    /// 生のビットマスクから構築する。
    pub const fn from_bits(bits: u32) -> FeatureSet {
        FeatureSet(bits)
    }

    /// 生のビットマスク。
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// 機能の並びから構築する。
    pub fn from_slice(features: &[Feature]) -> FeatureSet {
        let mut bits = 0u32;
        for f in features {
            bits |= f.bit();
        }
        FeatureSet(bits)
    }

    /// 機能名の並びから構築する。未知の名前は無視せずエラーにする。
    pub fn from_names(names: &[&str]) -> Result<FeatureSet, String> {
        let mut bits = 0u32;
        for n in names {
            match Feature::from_name(n) {
                Some(f) => bits |= f.bit(),
                None => return Err(format!("未知の CPU 機能名: {n}")),
            }
        }
        Ok(FeatureSet(bits))
    }

    /// 1 つ追加した集合を返す。
    pub const fn with(self, f: Feature) -> FeatureSet {
        FeatureSet(self.0 | f.bit())
    }

    /// 単一機能を含むか。
    pub const fn has(self, f: Feature) -> bool {
        self.0 & f.bit() != 0
    }

    /// `other` の機能を **すべて** 含むか(組み合わせ判定の中心)。
    pub const fn contains_all(self, other: FeatureSet) -> bool {
        self.0 & other.0 == other.0
    }

    /// `other` の機能を **いずれか** 含むか。
    pub const fn contains_any(self, other: FeatureSet) -> bool {
        self.0 & other.0 != 0
    }

    /// 積集合。
    pub const fn intersection(self, other: FeatureSet) -> FeatureSet {
        FeatureSet(self.0 & other.0)
    }

    /// 和集合。
    pub const fn union(self, other: FeatureSet) -> FeatureSet {
        FeatureSet(self.0 | other.0)
    }

    /// 差集合(`self` にあって `other` に無いもの)。
    pub const fn difference(self, other: FeatureSet) -> FeatureSet {
        FeatureSet(self.0 & !other.0)
    }

    /// 含まれる機能数。
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    /// 空集合か。
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// 含まれる機能を列挙する。
    pub fn iter(self) -> impl Iterator<Item = Feature> {
        ALL_FEATURES.into_iter().filter(move |f| self.has(*f))
    }

    /// スペース区切りの機能名。
    pub fn to_names(self) -> String {
        let v: Vec<&str> = self.iter().map(|f| f.name()).collect();
        if v.is_empty() {
            "(none)".to_string()
        } else {
            v.join(" ")
        }
    }
}

impl std::fmt::Display for FeatureSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_names())
    }
}

impl std::ops::BitOr for FeatureSet {
    type Output = FeatureSet;
    fn bitor(self, rhs: FeatureSet) -> FeatureSet {
        self.union(rhs)
    }
}

impl std::ops::BitAnd for FeatureSet {
    type Output = FeatureSet;
    fn bitand(self, rhs: FeatureSet) -> FeatureSet {
        self.intersection(rhs)
    }
}

impl From<Feature> for FeatureSet {
    fn from(f: Feature) -> FeatureSet {
        FeatureSet(f.bit())
    }
}

/// 実用上意味のある「命令セットの組み合わせの段階」。
///
/// 単一命令ではなく **組み合わせ** で定義されている点が要点。たとえば
/// [`IsaProfile::Avx512Vnni`] は AVX-512F + BW + VL + VNNI が **すべて**
/// 揃っている場合のみ成立する。列挙順はおおむね性能の高い順に並んでおり、
/// [`CpuCapabilities::at_least`] で「この組み合わせを満たしているか」を判定する。
/// ただし [`IsaProfile::Avx2Vnni`] だけは主系列からの分岐で、AVX-512 系は
/// AVX-VNNI(256bit 版)を要求しない点に注意。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IsaProfile {
    /// SIMD 前提なし(非 x86 を含む)。
    Baseline,
    /// SSE2(x86_64 では常に成立)。
    Sse2,
    /// SSSE3 + PCLMULQDQ(`vpshufb` / キャリーレス乗算が使える)。上位の
    /// AVX2 系プロファイルはこの段階を包含する(実在の AVX2 対応 CPU は
    /// いずれも PCLMULQDQ を備えているため)。
    Ssse3Pclmul,
    /// AVX2 のみ(FMA3 無し)。
    Avx2,
    /// AVX2 + FMA3(浮動小数点の積和が 1 命令)。
    Avx2Fma,
    /// AVX2 + FMA3 + AVX-VNNI(Alder Lake 以降の INT8 積和)。
    Avx2Vnni,
    /// AVX-512F + BW + VL(512bit 幅の整数/バイト演算)。
    Avx512,
    /// AVX-512F + BW + VL + VNNI(512bit 幅の INT8 積和)。
    Avx512Vnni,
}

/// 各プロファイルが要求する機能の組み合わせ。
fn profile_requirements(p: IsaProfile) -> FeatureSet {
    use Feature::*;
    match p {
        IsaProfile::Baseline => FeatureSet::EMPTY,
        IsaProfile::Sse2 => FeatureSet::from_slice(&[Sse2]),
        IsaProfile::Ssse3Pclmul => FeatureSet::from_slice(&[Sse2, Ssse3, Pclmulqdq]),
        IsaProfile::Avx2 => FeatureSet::from_slice(&[Sse2, Ssse3, Pclmulqdq, Avx2]),
        IsaProfile::Avx2Fma => FeatureSet::from_slice(&[Sse2, Ssse3, Pclmulqdq, Avx2, Fma]),
        IsaProfile::Avx2Vnni => FeatureSet::from_slice(&[Sse2, Ssse3, Pclmulqdq, Avx2, Fma, AvxVnni]),
        IsaProfile::Avx512 => FeatureSet::from_slice(&[
            Sse2, Ssse3, Pclmulqdq, Avx2, Fma, Avx512f, Avx512bw, Avx512vl,
        ]),
        IsaProfile::Avx512Vnni => FeatureSet::from_slice(&[
            Sse2, Ssse3, Pclmulqdq, Avx2, Fma, Avx512f, Avx512bw, Avx512vl, Avx512vnni,
        ]),
    }
}

/// 低い順に並べたプロファイル一覧。
pub const ALL_PROFILES: [IsaProfile; 8] = [
    IsaProfile::Baseline,
    IsaProfile::Sse2,
    IsaProfile::Ssse3Pclmul,
    IsaProfile::Avx2,
    IsaProfile::Avx2Fma,
    IsaProfile::Avx2Vnni,
    IsaProfile::Avx512,
    IsaProfile::Avx512Vnni,
];

impl IsaProfile {
    /// このプロファイルが要求する機能の組み合わせ。
    pub fn requirements(self) -> FeatureSet {
        profile_requirements(self)
    }

    /// 短い名前。
    pub const fn name(self) -> &'static str {
        match self {
            IsaProfile::Baseline => "baseline",
            IsaProfile::Sse2 => "sse2",
            IsaProfile::Ssse3Pclmul => "ssse3+pclmul",
            IsaProfile::Avx2 => "avx2",
            IsaProfile::Avx2Fma => "avx2+fma3",
            IsaProfile::Avx2Vnni => "avx2+fma3+vnni",
            IsaProfile::Avx512 => "avx512f+bw+vl",
            IsaProfile::Avx512Vnni => "avx512f+bw+vl+vnni",
        }
    }

    /// AVX-512 系(この開発機では実行未検証)のプロファイルか。
    pub const fn is_avx512(self) -> bool {
        matches!(self, IsaProfile::Avx512 | IsaProfile::Avx512Vnni)
    }
}

impl std::fmt::Display for IsaProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl CpuCapabilities {
    /// 検出済みフィールドを [`FeatureSet`] に畳み込む。
    pub fn feature_set(&self) -> FeatureSet {
        use Feature::*;
        let mut s = FeatureSet::EMPTY;
        for (on, f) in [
            (self.sse2, Sse2),
            (self.ssse3, Ssse3),
            (self.popcnt, Popcnt),
            (self.aes, Aes),
            (self.pclmulqdq, Pclmulqdq),
            (self.bmi1, Bmi1),
            (self.bmi2, Bmi2),
            (self.fma, Fma),
            (self.sha, Sha),
            (self.avx2, Avx2),
            (self.avx512f, Avx512f),
            (self.avx512bw, Avx512bw),
            (self.avx512vl, Avx512vl),
            (self.avx_vnni, AvxVnni),
            (self.avx512vnni, Avx512vnni),
            (self.gfni, Gfni),
            (self.vpclmulqdq, Vpclmulqdq),
        ] {
            if on {
                s = s.with(f);
            }
        }
        s
    }

    /// 指定した機能が **すべて** 揃っているか(組み合わせ判定)。
    ///
    /// ```
    /// use open_cpu::Feature;
    /// let caps = open_cpu::detect();
    /// let _ = caps.supports_all(&[Feature::Avx2, Feature::Fma]);
    /// ```
    pub fn supports_all(&self, features: &[Feature]) -> bool {
        self.feature_set()
            .contains_all(FeatureSet::from_slice(features))
    }

    /// 指定した機能を **いずれか** 持つか。
    pub fn supports_any(&self, features: &[Feature]) -> bool {
        self.feature_set()
            .contains_any(FeatureSet::from_slice(features))
    }

    /// 成立する最上位の [`IsaProfile`]。
    ///
    /// AVX-512 系は既定では選ばれない(この開発機では実行未検証のため)。
    /// 環境変数 `OPEN_CPU_ENABLE_AVX512=1` を設定した場合のみ候補に入る。
    /// 環境変数を無視した「CPU の素の実力」を知りたい場合は
    /// [`CpuCapabilities::isa_profile_raw`] を使う。
    pub fn isa_profile(&self) -> IsaProfile {
        let raw = self.isa_profile_raw();
        if raw.is_avx512() && !avx512_opt_in() {
            // AVX-512 を外した範囲での最上位へ降格する。
            return self.highest_profile_where(|p| !p.is_avx512());
        }
        raw
    }

    /// 環境変数による opt-in を無視した、純粋な検出結果としての最上位プロファイル。
    pub fn isa_profile_raw(&self) -> IsaProfile {
        self.highest_profile_where(|_| true)
    }

    fn highest_profile_where(&self, pred: impl Fn(IsaProfile) -> bool) -> IsaProfile {
        let have = self.feature_set();
        let mut best = IsaProfile::Baseline;
        for p in ALL_PROFILES {
            if pred(p) && have.contains_all(p.requirements()) {
                best = p;
            }
        }
        best
    }

    /// このプロファイル以上を満たしているか。
    pub fn at_least(&self, p: IsaProfile) -> bool {
        self.feature_set().contains_all(p.requirements())
    }

    /// 検出されているが、このクレートがまだ演算実装を持っていない機能。
    ///
    /// 「検出だけして使っていない」状態を呼び出し側から可視化するための API。
    pub fn detected_but_unused(&self) -> FeatureSet {
        self.feature_set().difference(implemented_features())
    }
}

/// このクレートが実際に **演算実装** で利用している機能。
///
/// 単なる検出フィールドの有無ではなく、その命令を使うコードパスが
/// 存在するものだけを列挙する(README の実装状況と一致させること)。
pub fn implemented_features() -> FeatureSet {
    use Feature::*;
    FeatureSet::from_slice(&[
        Sse2,
        Ssse3,
        Pclmulqdq,
        Popcnt,
        Bmi1,
        Bmi2,
        Avx2,
        Fma,
        Avx512f,
        Avx512bw,
    ])
}

/// `OPEN_CPU_ENABLE_AVX512=1` が設定されているか。
pub fn avx512_opt_in() -> bool {
    std::env::var("OPEN_CPU_ENABLE_AVX512")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// 候補のコードパスから、要求機能の組み合わせが揃う最初のものを選ぶ。
///
/// 候補は **優先度の高い順**(通常は速い順)に並べて渡す。どれも成立しない
/// 場合は `None` を返すので、呼び出し側でスカラー実装へフォールバックする。
///
/// ```
/// use open_cpu::{select, Feature};
/// let picked = select(&[
///     ("avx512", &[Feature::Avx512f, Feature::Avx512bw][..]),
///     ("avx2+fma", &[Feature::Avx2, Feature::Fma][..]),
///     ("sse2", &[Feature::Sse2][..]),
/// ]);
/// // 開発機(Zen2)では "avx2+fma" が選ばれる
/// let _ = picked;
/// ```
pub fn select<'a, T: Copy>(candidates: &[(T, &'a [Feature])]) -> Option<T> {
    let caps = detect();
    let have = caps.feature_set();
    for (tag, req) in candidates {
        let need = FeatureSet::from_slice(req);
        if need.iter().any(|f| {
            matches!(
                f,
                Feature::Avx512f
                    | Feature::Avx512bw
                    | Feature::Avx512vl
                    | Feature::Avx512vnni
            )
        }) && !avx512_opt_in()
        {
            continue;
        }
        if have.contains_all(need) {
            return Some(*tag);
        }
    }
    None
}

/// 現在の CPU における組み合わせディスパッチの状況を 1 行で表す。
pub fn isa_summary() -> String {
    let caps = detect();
    format!(
        "profile: {} (raw: {}) | features: {} | detected-but-unused: {}",
        caps.isa_profile(),
        caps.isa_profile_raw(),
        caps.feature_set(),
        caps.detected_but_unused()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_name_roundtrip() {
        for f in ALL_FEATURES {
            assert_eq!(Feature::from_name(f.name()), Some(f));
        }
        assert_eq!(Feature::from_name("AVX-VNNI"), Some(Feature::AvxVnni));
        assert_eq!(Feature::from_name("AVX_512F"), Some(Feature::Avx512f));
        assert_eq!(Feature::from_name("nosuch"), None);
    }

    #[test]
    fn set_operations() {
        let a = FeatureSet::from_slice(&[Feature::Avx2, Feature::Fma]);
        let b = FeatureSet::from_slice(&[Feature::Avx2]);
        assert!(a.contains_all(b));
        assert!(!b.contains_all(a));
        assert!(b.contains_any(a));
        assert_eq!(a.difference(b), FeatureSet::from(Feature::Fma));
        assert_eq!(a.len(), 2);
        assert_eq!((a & b), b);
        assert_eq!((b | FeatureSet::from(Feature::Fma)), a);
    }

    #[test]
    fn from_names_rejects_unknown() {
        assert!(FeatureSet::from_names(&["avx2", "fma"]).is_ok());
        assert!(FeatureSet::from_names(&["avx2", "zzz"]).is_err());
    }

    #[test]
    fn profiles_are_monotonic() {
        // 主系列(AVX-VNNI 分岐を除く)は上位が下位の要求を包含すること。
        let lineage = [
            IsaProfile::Baseline,
            IsaProfile::Sse2,
            IsaProfile::Ssse3Pclmul,
            IsaProfile::Avx2,
            IsaProfile::Avx2Fma,
            IsaProfile::Avx512,
            IsaProfile::Avx512Vnni,
        ];
        for w in lineage.windows(2) {
            let (lo, hi) = (w[0], w[1]);
            assert!(
                hi.requirements().contains_all(lo.requirements()),
                "{hi} が {lo} の要求を包含していない"
            );
        }
        // 分岐である Avx2Vnni は Avx2Fma を包含する。
        assert!(IsaProfile::Avx2Vnni
            .requirements()
            .contains_all(IsaProfile::Avx2Fma.requirements()));
        // AVX-512 系は AVX-VNNI(256bit 版)を要求しない(実在 CPU で
        // AVX-512 VNNI を持ちつつ AVX-VNNI を報告しないものがあるため)。
        assert!(!IsaProfile::Avx512Vnni.requirements().has(Feature::AvxVnni));
    }

    #[test]
    fn profile_matches_capabilities() {
        let caps = detect();
        let p = caps.isa_profile();
        assert!(caps.feature_set().contains_all(p.requirements()));
        // 既定では AVX-512 は選ばれない。
        if !avx512_opt_in() {
            assert!(!p.is_avx512());
        }
        println!("{}", isa_summary());
    }

    #[test]
    fn select_prefers_first_satisfiable() {
        let picked = select(&[
            ("impossible", &[Feature::Avx512vnni][..]),
            ("sse2", &[Feature::Sse2][..]),
        ]);
        if detect().sse2 {
            assert_eq!(picked, Some("sse2"));
        }
        // 何も要求しない候補は必ず成立する。
        assert_eq!(select(&[("any", &[][..])]), Some("any"));
    }
}
