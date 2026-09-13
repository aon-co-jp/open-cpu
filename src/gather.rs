//! バッチ化されたテーブルルックアップ(gather)。
//!
//! **由来・動機(2026-09-13)**: `open-directx`のFFv1レンジコーダー実装
//! (`directx-shader-translate::range_coder`)で、GPU側は「Nレーンの
//! invocationがそれぞれ並列に1個ずつテーブル値(状態遷移テーブル)を
//! 読み、1レーンだけが逐次処理を行う」というワークグループ共有メモリ
//! +バリア方式を実装し、32/64/128/256/512/1024/1536レーンで実GT730
//! ハードウェア検証した(`open-directx/PORTING.md`参照)。この
//! 「Nレーン並列lookup」パターンのCPU-SIMD側の直接対応物が、AVX2/
//! AVX-512の**gather命令**(1命令で複数の異なるインデックスからテーブル
//! 値をまとめて読む)である、というアイデアを実際にコードとして
//! 実装したもの。
//!
//! 提供する関数(詳細は各関数のdocコメント参照):
//! `gather_u8_scalar`(スカラー参照実装、全アーキテクチャ、正しさの基準)、
//! `gather_u8_avx2`(AVX2の`_mm256_i32gather_epi32`による8要素/命令の
//! バッチルックアップ、この開発機・AMD Ryzen 9 3950XでAVX2実行検証済み)、
//! `gather_u8`(検出結果に応じてAVX2/スカラーへ実行時ディスパッチする
//! 公開API)。
//!
//! **正直な開示**: AVX2には8bit要素を直接gatherする命令が無いため、
//! テーブルを事前に32bit整数へゼロ拡張してからgatherし、結果を再び
//! 8bitへ切り詰める(`expand_u8_table_to_i32`)——テーブルが256〜512
//! 要素程度(FFv1の`one_state`/`zero_state`/`zero_one_state`と同じ
//! 規模)であれば、この拡張コストは無視できる。AVX-512版(`vpgatherdd`
//! を16-wideで使う版)は、この開発機がAVX-512非搭載のため**未実装**
//! (`gf.rs`のAVX-512パスと同じ理由でコンパイル確認すらできない
//! ——将来AVX-512搭載機で追加する際の設計メモとしてこの制約を記録する)。

use crate::caps::detect;

/// スカラー参照実装。`indices[i]`が`table`の範囲外の場合は`0`を返す
/// (境界外アクセスによるパニック/UBを避けるための安全側フォールバック
/// ——AVX2版と挙動を一致させるため、AVX2版も同じ規約に従う)。
pub fn gather_u8_scalar(table: &[u8], indices: &[u32]) -> Vec<u8> {
    indices.iter().map(|&i| table.get(i as usize).copied().unwrap_or(0)).collect()
}

/// `table`(u8の配列)を、AVX2の`_mm256_i32gather_epi32`でそのまま使える
/// 形——各要素をi32へゼロ拡張した配列——へ変換する。
fn expand_u8_table_to_i32(table: &[u8]) -> Vec<i32> {
    table.iter().map(|&b| b as i32).collect()
}

/// AVX2の`_mm256_i32gather_epi32`を使い、8個のインデックスずつ
/// バッチでテーブル値を取得する。呼び出し前に`caps.avx2`を確認する
/// のは呼び出し側の責任(この関数自体は`#[target_feature(enable =
/// "avx2")]`が付いた`unsafe fn`——AVX2非搭載CPUで呼ぶと未定義動作)。
///
/// 範囲外インデックスは`0`を返す(`gather_u8_scalar`と同じ規約)——
/// gather命令自体はマスク無しでは範囲外を安全に扱えないため、
/// この関数はゼロ拡張したテーブルの前後に十分なパディングを
/// 内部で確保してから実際のgatherを行う(呼び出し側にAVX2固有の
/// 制約を意識させない)。
///
/// # Safety
/// 呼び出し側がAVX2を実際にサポートするCPU上で実行することを
/// 保証しなければならない(`std::is_x86_feature_detected!("avx2")`
/// または[`crate::detect`]で確認済みであること)。
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
pub unsafe fn gather_u8_avx2(table: &[u8], indices: &[u32]) -> Vec<u8> {
    use std::arch::x86_64::*;

    // gather命令はマスク無しでは常に8要素分読むため、範囲外インデックス
    // が来ても安全なよう、テーブルの後ろに8要素分のパディング(値0)を
    // 追加してから拡張する——範囲外インデックスは意図的にこのパディング
    // 領域を指すよう `padded_index` でクランプする。
    let padded_len = table.len() + 8;
    let mut padded_table = expand_u8_table_to_i32(table);
    padded_table.resize(padded_len, 0);

    let mut out = vec![0u8; indices.len()];
    // `as_chunks::<8>()`はこのプロジェクトのMSRVでは不安定機能のため、
    // 素直な`chunks_exact`を使う(clippyの`chunks_exact_to_as_chunks`
    // 提案は既知——`open-directx/crates/directx-shader-translate/src/
    // dxil.rs`にも同種の既存箇所がある)。
    #[allow(clippy::chunks_exact_to_as_chunks)]
    let mut chunks = indices.chunks_exact(8);
    let mut out_pos = 0usize;

    for chunk in &mut chunks {
        let idx = [
            clamp_index(chunk[0], table.len(), padded_len),
            clamp_index(chunk[1], table.len(), padded_len),
            clamp_index(chunk[2], table.len(), padded_len),
            clamp_index(chunk[3], table.len(), padded_len),
            clamp_index(chunk[4], table.len(), padded_len),
            clamp_index(chunk[5], table.len(), padded_len),
            clamp_index(chunk[6], table.len(), padded_len),
            clamp_index(chunk[7], table.len(), padded_len),
        ];
        // SAFETY: `_mm256_i32gather_epi32`はAVX2があれば安全に呼べる
        // (関数全体が`#[target_feature(enable = "avx2")]`)。
        // `idx`は全て`padded_table`の範囲内([0, padded_len))に
        // クランプ済みなので、読み出し自体は範囲外アクセスにならない。
        let idx_vec = unsafe { _mm256_loadu_si256(idx.as_ptr() as *const __m256i) };
        let gathered = unsafe { _mm256_i32gather_epi32(padded_table.as_ptr(), idx_vec, 4) };
        let mut lanes = [0i32; 8];
        unsafe { _mm256_storeu_si256(lanes.as_mut_ptr() as *mut __m256i, gathered) };
        for (k, &v) in lanes.iter().enumerate() {
            out[out_pos + k] = v as u8;
        }
        out_pos += 8;
    }

    for &i in chunks.remainder() {
        out[out_pos] = table.get(i as usize).copied().unwrap_or(0);
        out_pos += 1;
    }

    out
}

#[cfg(target_arch = "x86_64")]
fn clamp_index(i: u32, table_len: usize, padded_len: usize) -> i32 {
    if (i as usize) < table_len {
        i as i32
    } else {
        // パディング領域(値0)を指すようクランプする——範囲外なら
        // どのパディング要素を指しても結果は同じ(全て0)。
        (padded_len - 1) as i32
    }
}

/// [`crate::detect`]の結果に応じてAVX2/スカラーへ実行時ディスパッチする
/// 公開API。呼び出し側はCPU機能を意識する必要が無い。
pub fn gather_u8(table: &[u8], indices: &[u32]) -> Vec<u8> {
    #[cfg(target_arch = "x86_64")]
    {
        if detect().avx2 {
            // SAFETY: 直前に`detect().avx2`を確認済み。
            return unsafe { gather_u8_avx2(table, indices) };
        }
    }
    gather_u8_scalar(table, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_table(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 7 + 3) as u8).collect()
    }

    #[test]
    fn scalar_and_dispatched_agree_on_ffv1_sized_tables() {
        // open-directxのFFv1レンジコーダーが実際に使うテーブル規模
        // (one_state/zero_state=256要素、zero_one_state=512要素)と
        // 同じサイズで検証する。
        for table_len in [256usize, 512] {
            let table = make_table(table_len);
            // 8の倍数ぴったりではない個数(バッチ処理の残り処理も検証)、
            // かつテーブル範囲外のインデックスも混ぜる。
            let indices: Vec<u32> = (0..37u32).map(|i| (i * 13) % (table_len as u32 + 20)).collect();
            let expected = gather_u8_scalar(&table, &indices);
            let actual = gather_u8(&table, &indices);
            assert_eq!(actual, expected, "table_len={table_len}");
        }
    }

    #[test]
    #[cfg(target_arch = "x86_64")]
    fn avx2_path_matches_scalar_when_avx2_is_actually_available() {
        if !detect().avx2 {
            eprintln!("この開発機はAVX2非搭載のためスキップ");
            return;
        }
        let table = make_table(300);
        let indices: Vec<u32> = (0..64u32).map(|i| (i * 41 + 5) % 320).collect();
        let expected = gather_u8_scalar(&table, &indices);
        // SAFETY: 直前に`detect().avx2`を確認済み。
        let actual = unsafe { gather_u8_avx2(&table, &indices) };
        assert_eq!(actual, expected);
    }
}
