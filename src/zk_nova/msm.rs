//! Variable-time Pippenger multi-scalar multiplication.
//!
//! The result is a pure function of the inputs. Windows are independent, so they are distributed
//! over std threads; each window's sum is deterministic and the windows are combined in a fixed
//! order, so the output is identical for every thread count.

use core::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::curve::{Affine, Curve, Projective, ScalarOf};
use super::field::LIMBS;

/// Bit length used for every scalar window split (both Pasta scalar fields are 255-bit).
const SCALAR_BITS: usize = 255;
/// Largest window width in bits (bounds the bucket array at `2^15` entries per worker).
const MAX_WINDOW_BITS: usize = 15;
/// Below this many terms the MSM runs on the calling thread.
const PARALLEL_THRESHOLD: usize = 512;

/// Window index and sum pairs produced by one worker.
type WindowSums<C> = Vec<(usize, Projective<C>)>;

/// Errors from [`msm`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MsmError {
    /// `scalars` and `bases` have different lengths.
    LengthMismatch { scalars: usize, bases: usize },
    /// A worker thread panicked.
    WorkerFailed,
}

impl fmt::Display for MsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkerFailed => write!(f, "an msm worker thread failed"),
            Self::LengthMismatch { scalars, bases } => {
                write!(f, "msm got {scalars} scalars but {bases} bases")
            }
        }
    }
}

impl std::error::Error for MsmError {}

fn window_bits(n: usize) -> usize {
    if n < 32 {
        3
    } else {
        let log2 = (usize::BITS - 1 - n.leading_zeros()) as usize;
        log2.saturating_sub(3).clamp(3, MAX_WINDOW_BITS)
    }
}

/// The `width`-bit digit of `limbs` starting at bit `start`.
fn digit(limbs: &[u64; LIMBS], start: usize, width: usize) -> usize {
    let index = start / 64;
    let offset = start % 64;
    if index >= LIMBS {
        return 0;
    }
    let mut value = limbs[index] >> offset;
    if offset + width > 64 && index + 1 < LIMBS {
        value |= limbs[index + 1] << (64 - offset);
    }
    (value & ((1u64 << width) - 1)) as usize
}

fn window_sum<C: Curve>(
    scalars: &[[u64; LIMBS]],
    bases: &[Affine<C>],
    window: usize,
    width: usize,
) -> Projective<C> {
    let mut buckets = vec![Projective::<C>::identity(); (1usize << width) - 1];
    for (limbs, base) in scalars.iter().zip(bases) {
        let d = digit(limbs, window * width, width);
        if d != 0 {
            buckets[d - 1] = buckets[d - 1].add_affine(base);
        }
    }
    let mut running = Projective::<C>::identity();
    let mut total = Projective::<C>::identity();
    for bucket in buckets.iter().rev() {
        running = running.add_projective(bucket);
        total = total.add_projective(&running);
    }
    total
}

/// Computes `sum(scalars[i] * bases[i])` using all available hardware threads.
pub fn msm<C: Curve>(
    scalars: &[ScalarOf<C>],
    bases: &[Affine<C>],
) -> Result<Projective<C>, MsmError> {
    let available = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    msm_with_threads(scalars, bases, available)
}

/// Computes `sum(scalars[i] * bases[i])` on at most `max_threads` threads (`0` counts as `1`).
/// The result is identical for every thread count.
pub fn msm_with_threads<C: Curve>(
    scalars: &[ScalarOf<C>],
    bases: &[Affine<C>],
    max_threads: usize,
) -> Result<Projective<C>, MsmError> {
    if scalars.len() != bases.len() {
        return Err(MsmError::LengthMismatch {
            scalars: scalars.len(),
            bases: bases.len(),
        });
    }
    // Zero scalars and identity bases contribute nothing; drop them up front.
    let mut limbs = Vec::with_capacity(scalars.len());
    let mut points = Vec::with_capacity(scalars.len());
    for (scalar, base) in scalars.iter().zip(bases) {
        if scalar.is_zero() || base.is_identity() {
            continue;
        }
        limbs.push(scalar.to_limbs());
        points.push(*base);
    }
    let n = points.len();
    if n == 0 {
        return Ok(Projective::identity());
    }

    let width = window_bits(n);
    let windows = SCALAR_BITS.div_ceil(width);
    let threads = if n < PARALLEL_THRESHOLD {
        1
    } else {
        max_threads.clamp(1, windows)
    };

    let mut sums = vec![Projective::<C>::identity(); windows];
    if threads == 1 {
        for (window, slot) in sums.iter_mut().enumerate() {
            *slot = window_sum(&limbs, &points, window, width);
        }
    } else {
        let next = AtomicUsize::new(0);
        let done: Vec<Option<WindowSums<C>>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..threads)
                .map(|_| {
                    scope.spawn(|| {
                        let mut local = Vec::new();
                        loop {
                            let window = next.fetch_add(1, Ordering::Relaxed);
                            if window >= windows {
                                break;
                            }
                            local.push((window, window_sum(&limbs, &points, window, width)));
                        }
                        local
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().ok())
                .collect()
        });
        for worker in done {
            let worker = worker.ok_or(MsmError::WorkerFailed)?;
            for (window, sum) in worker {
                sums[window] = sum;
            }
        }
    }

    let mut total = Projective::<C>::identity();
    for sum in sums.iter().rev() {
        for _ in 0..width {
            total = total.double();
        }
        total = total.add_projective(sum);
    }
    Ok(total)
}
