//! スカラー実装と SIMD 実装の実測比較(簡易ベンチ、`std::time::Instant`)。
//!
//! 実行: `cargo run --release --example bench`

use std::time::Instant;

fn main() {
    println!("{}", open_cpu::runtime_summary());

    const LEN: usize = 4 * 1024 * 1024; // 4 MiB
    const ITERS: usize = 50;
    let src: Vec<u8> = (0..LEN)
        .map(|i| ((i as u32).wrapping_mul(2654435761) >> 13) as u8)
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

    // --- P パリティ(単純 XOR) ---
    println!();
    let t = Instant::now();
    for _ in 0..ITERS {
        open_cpu::gf_xor_scalar(&mut dst, &src);
    }
    let xs = t.elapsed().as_secs_f64();
    let t = Instant::now();
    for _ in 0..ITERS {
        open_cpu::gf_xor(&mut dst, &src);
    }
    let xd = t.elapsed().as_secs_f64();
    println!(
        "xor scalar   : {:>8.3} ms  {:>9.2} MiB/s",
        xs * 1000.0,
        bytes / xs / (1024.0 * 1024.0)
    );
    println!(
        "xor dispatch : {:>8.3} ms  {:>9.2} MiB/s  ({:.2}x vs scalar)",
        xd * 1000.0,
        bytes / xd / (1024.0 * 1024.0),
        xs / xd
    );

    // --- ホーナー法(acc = acc*2 ^ src、Q シンドローム) ---
    println!();
    let t = Instant::now();
    for _ in 0..ITERS {
        open_cpu::gf_mul_pow2_xor_scalar(&mut dst, &src, 1);
    }
    let hs = t.elapsed().as_secs_f64();
    let t = Instant::now();
    for _ in 0..ITERS {
        open_cpu::gf_mul2_xor(&mut dst, &src);
    }
    let hd = t.elapsed().as_secs_f64();
    println!(
        "horner scalar  : {:>8.3} ms  {:>9.2} MiB/s",
        hs * 1000.0,
        bytes / hs / (1024.0 * 1024.0)
    );
    println!(
        "horner dispatch: {:>8.3} ms  {:>9.2} MiB/s  ({:.2}x vs scalar)",
        hd * 1000.0,
        bytes / hd / (1024.0 * 1024.0),
        hs / hd
    );

    std::hint::black_box(&dst);

    bench_float();
    bench_bits();
    bench_pext();
}

/// pext/pdep が本当に速いのかを実測する(Zen〜Zen 2 ではマイクロコードで遅い)。
fn bench_pext() {
    const REPS: usize = 20_000_000;
    let caps = open_cpu::detect();
    let (vendor, family) = open_cpu::vendor_family();
    println!();
    println!(
        "--- pext/pdep (vendor: {vendor:?} family: {family:#x} | bmi2 bit: {} | fast_bmi2(): {}) ---",
        caps.bmi2,
        caps.fast_bmi2()
    );
    let mask = 0x5555_5555_5555_5555u64;

    let t = Instant::now();
    let mut acc = 0u64;
    for i in 0..REPS as u64 {
        acc ^= open_cpu::extract_bits_scalar(i.wrapping_mul(0x9E3779B97F4A7C15), mask);
    }
    let ss = t.elapsed().as_secs_f64();

    #[cfg(target_arch = "x86_64")]
    let hw = if caps.bmi2 {
        let t = Instant::now();
        let mut acc2 = 0u64;
        for i in 0..REPS as u64 {
            acc2 ^= unsafe {
                std::arch::x86_64::_pext_u64(i.wrapping_mul(0x9E3779B97F4A7C15), mask)
            };
        }
        std::hint::black_box(acc2);
        Some(t.elapsed().as_secs_f64())
    } else {
        None
    };
    #[cfg(not(target_arch = "x86_64"))]
    let hw: Option<f64> = None;

    std::hint::black_box(acc);
    println!("pext scalar : {:>8.3} ms", ss * 1000.0);
    match hw {
        Some(hs) => println!(
            "pext bmi2   : {:>8.3} ms  ({:.2}x vs scalar{})",
            hs * 1000.0,
            ss / hs,
            if ss / hs < 1.0 { " ← 遅い!スカラーを選ぶべき" } else { "" }
        ),
        None => println!("pext bmi2   : BMI2 非搭載のため計測不可"),
    }
}

/// dot_f32 / axpy_f32 の実測(スカラー vs 組み合わせディスパッチ)。
fn bench_float() {
    const N: usize = 1 << 16; // 65536 要素 = 256 KiB
    const REPS: usize = 2000;
    let a: Vec<f32> = (0..N).map(|i| (i % 97) as f32 * 0.01).collect();
    let b: Vec<f32> = (0..N).map(|i| (i % 89) as f32 * 0.02).collect();

    println!();
    println!("--- float kernels (impl: {}) ---", open_cpu::selected_float_impl());

    let t = Instant::now();
    let mut s0 = 0f32;
    for _ in 0..REPS {
        s0 += open_cpu::dot_f32_scalar(&a, &b);
    }
    let ds = t.elapsed().as_secs_f64();

    let t = Instant::now();
    let mut s1 = 0f32;
    for _ in 0..REPS {
        s1 += open_cpu::dot_f32(&a, &b);
    }
    let dd = t.elapsed().as_secs_f64();
    let gflops = |sec: f64| (2.0 * N as f64 * REPS as f64) / sec / 1e9;
    println!(
        "dot scalar     : {:>8.3} ms  {:>6.2} GFLOP/s",
        ds * 1000.0,
        gflops(ds)
    );
    println!(
        "dot dispatch   : {:>8.3} ms  {:>6.2} GFLOP/s  ({:.2}x vs scalar)",
        dd * 1000.0,
        gflops(dd),
        ds / dd
    );
    println!("  (sum check: scalar {s0:.3} / dispatch {s1:.3})");

    let mut acc = vec![0f32; N];
    let t = Instant::now();
    for _ in 0..REPS {
        open_cpu::axpy_f32_scalar(&mut acc, &b, 1.000001);
    }
    let xs = t.elapsed().as_secs_f64();
    let mut acc2 = vec![0f32; N];
    let t = Instant::now();
    for _ in 0..REPS {
        open_cpu::axpy_f32(&mut acc2, &b, 1.000001);
    }
    let xd = t.elapsed().as_secs_f64();
    println!(
        "axpy scalar    : {:>8.3} ms  {:>6.2} GFLOP/s",
        xs * 1000.0,
        gflops(xs)
    );
    println!(
        "axpy dispatch  : {:>8.3} ms  {:>6.2} GFLOP/s  ({:.2}x vs scalar)",
        xd * 1000.0,
        gflops(xd),
        xs / xd
    );
    std::hint::black_box((&acc, &acc2));
}

/// popcount / hamming の実測。
fn bench_bits() {
    const N: usize = 4 << 20;
    const REPS: usize = 50;
    let a: Vec<u8> = (0..N).map(|i| (i % 251) as u8).collect();
    let b: Vec<u8> = (0..N).map(|i| (i % 253) as u8).collect();
    let bytes = (N * REPS) as f64;

    println!();
    println!("--- bit kernels ({}) ---", open_cpu::bit_impl_summary());
    let t = Instant::now();
    let mut c0 = 0u64;
    for _ in 0..REPS {
        c0 += open_cpu::popcount_bytes_scalar(&a);
    }
    let ps = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let mut c1 = 0u64;
    for _ in 0..REPS {
        c1 += open_cpu::popcount_bytes(&a);
    }
    let pd = t.elapsed().as_secs_f64();
    assert_eq!(c0, c1);
    println!(
        "popcount scalar  : {:>8.3} ms  {:>9.2} MiB/s",
        ps * 1000.0,
        bytes / ps / (1024.0 * 1024.0)
    );
    println!(
        "popcount dispatch: {:>8.3} ms  {:>9.2} MiB/s  ({:.2}x vs scalar)",
        pd * 1000.0,
        bytes / pd / (1024.0 * 1024.0),
        ps / pd
    );

    let t = Instant::now();
    let mut h0 = 0u64;
    for _ in 0..REPS {
        h0 += open_cpu::hamming_distance_scalar(&a, &b);
    }
    let hs = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let mut h1 = 0u64;
    for _ in 0..REPS {
        h1 += open_cpu::hamming_distance(&a, &b);
    }
    let hd = t.elapsed().as_secs_f64();
    assert_eq!(h0, h1);
    println!(
        "hamming scalar   : {:>8.3} ms  {:>9.2} MiB/s",
        hs * 1000.0,
        bytes / hs / (1024.0 * 1024.0)
    );
    println!(
        "hamming dispatch : {:>8.3} ms  {:>9.2} MiB/s  ({:.2}x vs scalar)",
        hd * 1000.0,
        bytes / hd / (1024.0 * 1024.0),
        hs / hd
    );
}
