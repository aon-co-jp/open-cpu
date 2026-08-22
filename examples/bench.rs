//! スカラー実装と SIMD 実装の実測比較(簡易ベンチ、`std::time::Instant`)。
//!
//! 実行: `cargo run --release --example bench`

use std::time::Instant;

fn main() {
    println!("{}", open_cpu::runtime_summary());

    const LEN: usize = 4 * 1024 * 1024; // 4 MiB
    const ITERS: usize = 50;
    let src: Vec<u8> = (0..LEN)
        .map(|i| (i as u32 * 2654435761 >> 13) as u8)
        .collect();
    let mut dst = vec![0u8; LEN];
    let factor: u8 = 0x8d;

    let bytes = (LEN * ITERS) as f64;

    // --- scalar ---
    let t = Instant::now();
    for _ in 0..ITERS {
        open_cpu::gf_mul_parity_scalar(&mut dst, &src, factor);
    }
    let scalar = t.elapsed().as_secs_f64();
    println!(
        "scalar   : {:>8.3} ms  {:>8.2} MiB/s",
        scalar * 1000.0,
        bytes / scalar / (1024.0 * 1024.0)
    );

    let caps = open_cpu::detect();

    // --- pclmulqdq ---
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if caps.pclmulqdq && caps.ssse3 {
        let t = Instant::now();
        for _ in 0..ITERS {
            unsafe { open_cpu::gf_mul_parity_pclmul(&mut dst, &src, factor) };
        }
        let e = t.elapsed().as_secs_f64();
        println!(
            "pclmulqdq: {:>8.3} ms  {:>8.2} MiB/s  ({:.2}x vs scalar)",
            e * 1000.0,
            bytes / e / (1024.0 * 1024.0),
            scalar / e
        );
    }

    // --- avx2 ---
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if caps.avx2 {
        let t = Instant::now();
        for _ in 0..ITERS {
            unsafe { open_cpu::gf_mul_parity_avx2(&mut dst, &src, factor) };
        }
        let e = t.elapsed().as_secs_f64();
        println!(
            "avx2     : {:>8.3} ms  {:>8.2} MiB/s  ({:.2}x vs scalar)",
            e * 1000.0,
            bytes / e / (1024.0 * 1024.0),
            scalar / e
        );
    }

    // AVX-512 は開発機非搭載のため実測不可(検出されたときだけ実行)。
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if caps.avx512f && caps.avx512bw {
        let t = Instant::now();
        for _ in 0..ITERS {
            unsafe { open_cpu::gf_mul_parity_avx512(&mut dst, &src, factor) };
        }
        let e = t.elapsed().as_secs_f64();
        println!(
            "avx512   : {:>8.3} ms  {:>8.2} MiB/s  ({:.2}x vs scalar)",
            e * 1000.0,
            bytes / e / (1024.0 * 1024.0),
            scalar / e
        );
    } else {
        println!("avx512   : このCPUでは未搭載のため実測不可(未検証)");
    }

    std::hint::black_box(&dst);
}
