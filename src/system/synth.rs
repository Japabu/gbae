use std::f64::consts::{PI, TAU};
use std::sync::OnceLock;

use super::cpu::CPU_FREQUENCY;

const HALF_WIDTH: usize = 16;
const TAPS: usize = HALF_WIDTH * 2;
const PHASES: usize = 256;
const CUTOFF: f64 = 0.92;
const KAISER_BETA: f64 = 8.0;
const BASS_CUTOFF_HZ: f64 = 15.0;
const OUTPUT_SCALE: f32 = 64.0;
const MAX_OUTPUT_SECONDS: usize = 2;
const BUFFER_SLACK: usize = 256;

type Weights = [f32; TAPS];

struct Kernel {
    rows: Vec<Weights>,
}

fn kernel() -> &'static Kernel {
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
        term *= (x / (2.0 * f64::from(k))).powi(2);
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
        let rows = (0..=PHASES)
            .map(|phase| {
                let fraction = phase as f64 / PHASES as f64;
                let taps: Vec<f64> = (0..TAPS)
                    .map(|i| {
                        let x = i as f64 - (HALF_WIDTH - 1) as f64 - fraction;
                        CUTOFF * sinc(CUTOFF * x) * kaiser(x / HALF_WIDTH as f64)
                    })
                    .collect();
                let total: f64 = taps.iter().sum();
                let mut row = [0f32; TAPS];
                for (weight, tap) in row.iter_mut().zip(&taps) {
                    *weight = (tap / total) as f32;
                }
                row
            })
            .collect();
        Kernel { rows }
    }

    fn weights(&self, fraction: f64) -> &Weights {
        &self.rows[(fraction * PHASES as f64).round() as usize]
    }
}

pub struct Synth {
    sample_rate: u32,
    samples_per_cycle: f64,
    position: f64,
    buffer: Vec<f32>,
    sum: f32,
    leak: f32,
    output: Vec<i16>,
}

impl Synth {
    pub fn new(sample_rate: u32) -> Synth {
        let mut synth = Synth {
            sample_rate,
            samples_per_cycle: 0.0,
            position: 0.0,
            buffer: Vec::new(),
            sum: 0.0,
            leak: 0.0,
            output: Vec::new(),
        };
        synth.set_sample_rate(sample_rate);
        synth
    }

    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        self.sample_rate = sample_rate;
        self.samples_per_cycle = f64::from(sample_rate) / CPU_FREQUENCY as f64;
        self.leak = (1.0 - (-TAU * BASS_CUTOFF_HZ / f64::from(sample_rate)).exp()) as f32;
        self.clear();
    }

    pub fn clear(&mut self) {
        self.position = 0.0;
        self.buffer.clear();
        self.sum = 0.0;
        self.output.clear();
    }

    fn time(&self, cycles: u32) -> f64 {
        self.position + f64::from(cycles) * self.samples_per_cycle
    }

    pub fn phase(&self, cycles: u32) -> f64 {
        self.time(cycles).fract()
    }

    pub fn set_phase(&mut self, phase: f64) {
        self.clear();
        if phase.is_finite() {
            self.position = phase.fract().abs();
        }
    }

    pub fn available(&self, cycles: u32) -> usize {
        self.output.len() + self.time(cycles).floor() as usize
    }

    fn reserve(&mut self, samples: usize) {
        if self.buffer.len() < samples + TAPS {
            self.buffer.resize(samples + TAPS + BUFFER_SLACK, 0.0);
        }
    }

    pub fn add_delta(&mut self, cycles: u32, delta: f32) {
        let time = self.time(cycles);
        let index = time.floor();
        let weights = kernel().weights(time - index);
        let index = index as usize;
        self.reserve(index);
        for (slot, weight) in self.buffer[index..index + TAPS].iter_mut().zip(weights) {
            *slot += weight * delta;
        }
    }

    pub fn end_frame(&mut self, cycles: u32) {
        let time = self.time(cycles);
        let count = time.floor() as usize;
        self.position = time - count as f64;
        self.reserve(count);
        for value in self.buffer.drain(..count) {
            self.sum += value;
            self.sum -= self.sum * self.leak;
            self.output.push((self.sum * OUTPUT_SCALE).round().clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16);
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

    const STEP: f32 = 64.0;
    const STEP_OUTPUT: i32 = (STEP * OUTPUT_SCALE) as i32;

    #[test]
    fn test_kernel_rows_sum_to_unity() {
        for row in &kernel().rows {
            assert!((row.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn test_step_lands_on_the_expected_sample() {
        let mut synth = Synth::new(48_000);
        synth.add_delta(10_000, STEP);
        synth.end_frame(100_000);
        let output = synth.take();
        let expected = 10_000.0 * 48_000.0 / CPU_FREQUENCY as f64 + (HALF_WIDTH - 1) as f64;
        let edge = output
            .windows(2)
            .position(|pair| i32::from(pair[0]) < STEP_OUTPUT / 2 && i32::from(pair[1]) >= STEP_OUTPUT / 2)
            .unwrap();
        assert!((edge as f64 - expected).abs() <= 1.0, "edge at {} expected {:.1}", edge, expected);
        assert!(i32::from(output[edge - 8]).abs() < STEP_OUTPUT / 16);
        assert!((i32::from(output[edge + 8]) - STEP_OUTPUT).abs() < STEP_OUTPUT / 16);
    }

    #[test]
    fn test_dc_offset_decays() {
        let mut synth = Synth::new(48_000);
        synth.add_delta(0, STEP);
        synth.end_frame(CPU_FREQUENCY as u32 / 2);
        let output = synth.take();
        assert!(i32::from(output[100]) > STEP_OUTPUT / 2);
        assert!(i32::from(*output.last().unwrap()).abs() < STEP_OUTPUT / 100);
    }

    #[test]
    fn test_sample_count_follows_the_rate() {
        let mut synth = Synth::new(44_100);
        synth.end_frame(CPU_FREQUENCY as u32);
        assert_eq!(synth.take().len(), 44_100);
    }
}
