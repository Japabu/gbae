use std::f64::consts::PI;
use std::sync::OnceLock;

use super::cpu::CPU_FREQUENCY;

const HALF_WIDTH: usize = 16;
pub const TAPS: usize = HALF_WIDTH * 2;
const PHASE_BITS: u32 = 6;
const PHASES: usize = 1 << PHASE_BITS;
const LERP_BITS: u32 = 10;
pub const FRACTION_BITS: u32 = PHASE_BITS + LERP_BITS;
const TIME_BITS: u32 = 32;
pub const KERNEL_SHIFT: u32 = 15;
pub const LEVEL_BITS: u32 = 14;
const OUTPUT_SHIFT: u32 = KERNEL_SHIFT + LEVEL_BITS - 16;
const CUTOFF: f64 = 0.92;
const KAISER_BETA: f64 = 8.0;
const BASS_SHIFT: u32 = 9;
const MAX_OUTPUT_SECONDS: usize = 2;
const BUFFER_SLACK: usize = 256;

pub type Weights = [i32; TAPS];

pub struct Kernel {
    rows: Vec<Weights>,
}

pub fn kernel() -> &'static Kernel {
    static KERNEL: OnceLock<Kernel> = OnceLock::new();
    KERNEL.get_or_init(Kernel::new)
}

fn sinc(x: f64) -> f64 {
    if x == 0.0 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

fn bessel_i0(x: f64) -> f64 {
    let mut sum = 1.0;
    let mut term = 1.0;
    for k in 1..40 {
        term *= (x / (2.0 * k as f64)).powi(2);
        sum += term;
        if term < 1e-12 * sum {
            break;
        }
    }
    sum
}

fn kaiser(t: f64) -> f64 {
    if t.abs() >= 1.0 {
        0.0
    } else {
        bessel_i0(KAISER_BETA * (1.0 - t * t).sqrt()) / bessel_i0(KAISER_BETA)
    }
}

impl Kernel {
    fn new() -> Kernel {
        let rows = (0..PHASES + 2)
            .map(|phase| {
                let fraction = phase as f64 / PHASES as f64;
                let taps: Vec<f64> = (0..TAPS)
                    .map(|i| {
                        let x = i as f64 - (HALF_WIDTH - 1) as f64 - fraction;
                        CUTOFF * sinc(CUTOFF * x) * kaiser(x / HALF_WIDTH as f64)
                    })
                    .collect();
                let total: f64 = taps.iter().sum();
                let mut row = [0i32; TAPS];
                for (fixed, tap) in row.iter_mut().zip(&taps) {
                    *fixed = (tap / total * (1 << KERNEL_SHIFT) as f64).round() as i32;
                }
                let peak = (0..TAPS).max_by_key(|i| row[*i]).unwrap();
                row[peak] += (1 << KERNEL_SHIFT) - row.iter().sum::<i32>();
                row
            })
            .collect();
        Kernel { rows }
    }

    pub fn weights(&self, fraction: u32) -> Weights {
        let phase = (fraction >> LERP_BITS) as usize;
        let lerp = (fraction & ((1 << LERP_BITS) - 1)) as i32;
        let (a, b) = (&self.rows[phase], &self.rows[phase + 1]);
        let mut row = [0i32; TAPS];
        for i in 0..TAPS {
            row[i] = (a[i] * ((1 << LERP_BITS) - lerp) + b[i] * lerp) >> LERP_BITS;
        }
        row
    }
}

pub struct Synth {
    sample_rate: u32,
    factor: u64,
    offset: u64,
    buffer: Vec<i32>,
    sum: i32,
    output: Vec<i16>,
}

impl Synth {
    pub fn new(sample_rate: u32) -> Synth {
        let mut synth = Synth {
            sample_rate,
            factor: 0,
            offset: 0,
            buffer: Vec::new(),
            sum: 0,
            output: Vec::new(),
        };
        synth.set_sample_rate(sample_rate);
        synth
    }

    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate;
        self.factor = ((sample_rate as u64) << TIME_BITS) / CPU_FREQUENCY;
        self.clear();
    }

    pub fn clear(&mut self) {
        self.offset = 0;
        self.buffer.clear();
        self.sum = 0;
        self.output.clear();
    }

    pub fn phase(&self, cycles: u32) -> u64 {
        (self.offset + cycles as u64 * self.factor) & ((1 << TIME_BITS) - 1)
    }

    pub fn set_phase(&mut self, phase: u64) {
        self.clear();
        self.offset = phase & ((1 << TIME_BITS) - 1);
    }

    pub fn available(&self, cycles: u32) -> usize {
        self.output.len() + ((self.offset + cycles as u64 * self.factor) >> TIME_BITS) as usize
    }

    pub fn add_delta(&mut self, cycles: u32, delta: i32) {
        let time = self.offset + cycles as u64 * self.factor;
        let index = (time >> TIME_BITS) as usize;
        let fraction = ((time >> (TIME_BITS - FRACTION_BITS)) & ((1 << FRACTION_BITS) - 1)) as u32;
        if self.buffer.len() < index + TAPS {
            self.buffer.resize(index + TAPS + BUFFER_SLACK, 0);
        }
        let weights = kernel().weights(fraction);
        for (slot, weight) in self.buffer[index..index + TAPS].iter_mut().zip(weights) {
            *slot += weight * delta;
        }
    }

    pub fn end_frame(&mut self, cycles: u32) {
        let time = self.offset + cycles as u64 * self.factor;
        let count = (time >> TIME_BITS) as usize;
        self.offset = time & ((1 << TIME_BITS) - 1);
        if self.buffer.len() < count + TAPS {
            self.buffer.resize(count + TAPS + BUFFER_SLACK, 0);
        }
        for value in self.buffer.drain(..count) {
            self.sum += value;
            self.sum -= self.sum >> BASS_SHIFT;
            self.output.push((self.sum >> OUTPUT_SHIFT).clamp(i16::MIN as i32, i16::MAX as i32) as i16);
        }
        let limit = MAX_OUTPUT_SECONDS * self.sample_rate as usize;
        if self.output.len() > limit {
            self.output.drain(..self.output.len() - limit);
        }
    }

    pub fn take(&mut self) -> Vec<i16> {
        std::mem::take(&mut self.output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: i32 = 1 << (LEVEL_BITS - 2);
    const STEP_OUTPUT: i32 = STEP << (16 - LEVEL_BITS);

    #[test]
    fn test_kernel_rows_sum_to_unity() {
        for row in &kernel().rows {
            assert_eq!(row.iter().sum::<i32>(), 1 << KERNEL_SHIFT);
        }
    }

    #[test]
    fn test_step_lands_on_the_expected_sample() {
        let mut synth = Synth::new(48_000);
        synth.add_delta(10_000, STEP);
        synth.end_frame(100_000);
        let output = synth.take();
        let expected = 10_000.0 * 48_000.0 / CPU_FREQUENCY as f64 + (HALF_WIDTH - 1) as f64;
        let edge = output.windows(2).position(|pair| (pair[0] as i32) < STEP_OUTPUT / 2 && pair[1] as i32 >= STEP_OUTPUT / 2).unwrap();
        assert!((edge as f64 - expected).abs() <= 1.0, "edge at {} expected {:.1}", edge, expected);
        assert!((output[edge - 8] as i32).abs() < STEP_OUTPUT / 16);
        assert!((output[edge + 8] as i32 - STEP_OUTPUT).abs() < STEP_OUTPUT / 16);
    }

    #[test]
    fn test_dc_offset_decays() {
        let mut synth = Synth::new(48_000);
        synth.add_delta(0, STEP);
        synth.end_frame(CPU_FREQUENCY as u32 / 2);
        let output = synth.take();
        assert!(output[100] as i32 > STEP_OUTPUT / 2);
        assert!((*output.last().unwrap() as i32).abs() < STEP_OUTPUT / 100);
    }
}
