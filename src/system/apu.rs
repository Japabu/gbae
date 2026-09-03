use std::collections::VecDeque;

use crate::bits::Bits;

use super::{
    cpu::CPU_FREQUENCY,
    state::{Reader, StateError, Writer},
    synth::Synth,
};

pub const SAMPLE_RATE: u32 = 48_000;
const FRAME_SEQUENCER_CYCLES: u32 = 32_768;
const GRID_CYCLES: u32 = 64;
const FLUSH_CYCLES: u32 = CPU_FREQUENCY as u32 / 8;
const FIFO_CAPACITY: usize = 32;
const FIFO_REFILL_THRESHOLD: usize = 16;
const DEFAULT_BIAS: u16 = 0x200;
const DAC_CENTER: f32 = 512.0;
const DAC_MAXIMUM: f32 = 1023.0;
const PSG_FULL_SCALE: f32 = 4.0 / 7.0;
const MAXIMUM_FREQUENCY: u16 = 2047;

pub const FIFO_A: u32 = 0x0400_00A0;
pub const FIFO_B: u32 = 0x0400_00A4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

impl Side {
    const ALL: [Side; 2] = [Side::Left, Side::Right];
}

#[derive(Debug, Clone, Copy, Default)]
struct Envelope {
    initial_volume: u8,
    increase: bool,
    step_time: u8,
    volume: u8,
    counter: u8,
}

impl Envelope {
    fn write(&mut self, value: u16) {
        self.initial_volume = value.bits(12..16) as u8;
        self.increase = value.bit(11);
        self.step_time = value.bits(8..11) as u8;
    }

    fn read(&self) -> u16 {
        u16::from(self.initial_volume) << 12 | u16::from(self.increase) << 11 | u16::from(self.step_time) << 8
    }

    fn trigger(&mut self) {
        self.volume = self.initial_volume;
        self.counter = self.step_time;
    }

    fn dac_enabled(&self) -> bool {
        self.initial_volume != 0 || self.increase
    }

    fn tick(&mut self) {
        if self.step_time == 0 {
            return;
        }
        self.counter = self.counter.saturating_sub(1);
        if self.counter == 0 {
            self.counter = self.step_time;
            self.volume = if self.increase { (self.volume + 1).min(15) } else { self.volume.saturating_sub(1) };
        }
    }

    fn save_state(&self, writer: &mut Writer) {
        writer.u8(self.initial_volume);
        writer.bool(self.increase);
        writer.u8(self.step_time);
        writer.u8(self.volume);
        writer.u8(self.counter);
    }

    fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.initial_volume = reader.u8()?;
        self.increase = reader.bool()?;
        self.step_time = reader.u8()?;
        self.volume = reader.u8()?;
        self.counter = reader.u8()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Length {
    enabled: bool,
    counter: u16,
    maximum: u16,
}

impl Length {
    fn new(maximum: u16) -> Length {
        Length { enabled: false, counter: 0, maximum }
    }

    fn write(&mut self, value: u16) {
        self.counter = self.maximum - (value & (self.maximum - 1));
    }

    fn trigger(&mut self) {
        if self.counter == 0 {
            self.counter = self.maximum;
        }
    }

    fn tick(&mut self) -> bool {
        if self.enabled && self.counter > 0 {
            self.counter -= 1;
            self.counter == 0
        } else {
            false
        }
    }

    fn save_state(&self, writer: &mut Writer) {
        writer.bool(self.enabled);
        writer.u16(self.counter);
    }

    fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.enabled = reader.bool()?;
        self.counter = reader.u16()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SweepStep {
    Idle,
    Frequency(u16),
    Overflow,
}

#[derive(Debug, Clone, Copy, Default)]
struct Sweep {
    shift: u8,
    decrease: bool,
    time: u8,
    counter: u8,
    shadow_frequency: u16,
    enabled: bool,
}

impl Sweep {
    fn write(&mut self, value: u16) {
        self.shift = value.bits(0..3) as u8;
        self.decrease = value.bit(3);
        self.time = value.bits(4..7) as u8;
    }

    fn read(&self) -> u16 {
        u16::from(self.shift) | u16::from(self.decrease) << 3 | u16::from(self.time) << 4
    }

    fn period(&self) -> u8 {
        if self.time == 0 {
            8
        } else {
            self.time
        }
    }

    fn trigger(&mut self, frequency: u16) -> bool {
        self.shadow_frequency = frequency;
        self.counter = self.period();
        self.enabled = self.time != 0 || self.shift != 0;
        self.shift != 0 && self.next_frequency() > MAXIMUM_FREQUENCY
    }

    fn next_frequency(&self) -> u16 {
        let delta = self.shadow_frequency >> self.shift;
        if self.decrease {
            self.shadow_frequency.wrapping_sub(delta)
        } else {
            self.shadow_frequency + delta
        }
    }

    fn tick(&mut self) -> SweepStep {
        self.counter = self.counter.saturating_sub(1);
        if self.counter != 0 {
            return SweepStep::Idle;
        }
        self.counter = self.period();
        if !self.enabled || self.time == 0 {
            return SweepStep::Idle;
        }
        let next = self.next_frequency();
        if next > MAXIMUM_FREQUENCY {
            return SweepStep::Overflow;
        }
        if self.shift == 0 {
            return SweepStep::Idle;
        }
        self.shadow_frequency = next;
        if self.next_frequency() > MAXIMUM_FREQUENCY {
            SweepStep::Overflow
        } else {
            SweepStep::Frequency(next)
        }
    }

    fn save_state(&self, writer: &mut Writer) {
        writer.u8(self.shift);
        writer.bool(self.decrease);
        writer.u8(self.time);
        writer.u8(self.counter);
        writer.u16(self.shadow_frequency);
        writer.bool(self.enabled);
    }

    fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.shift = reader.u8()?;
        self.decrease = reader.bool()?;
        self.time = reader.u8()?;
        self.counter = reader.u8()?;
        self.shadow_frequency = reader.u16()?;
        self.enabled = reader.bool()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Duty {
    #[default]
    Eighth,
    Quarter,
    Half,
    ThreeQuarters,
}

impl Duty {
    const ALL: [Duty; 4] = [Duty::Eighth, Duty::Quarter, Duty::Half, Duty::ThreeQuarters];

    fn from_bits(bits: u16) -> Duty {
        Duty::ALL[bits.bits(0..2) as usize]
    }

    fn bits(self) -> u16 {
        self as u16
    }

    fn pattern(self) -> u8 {
        match self {
            Duty::Eighth => 0b0000_0001,
            Duty::Quarter => 0b0000_0011,
            Duty::Half => 0b0000_1111,
            Duty::ThreeQuarters => 0b1111_1100,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Square {
    sweep: Sweep,
    duty: Duty,
    length: Length,
    envelope: Envelope,
    frequency: u16,
    enabled: bool,
    phase: u8,
    cycles: i32,
}

impl Square {
    fn new() -> Square {
        Square {
            length: Length::new(64),
            ..Default::default()
        }
    }

    fn period(&self) -> i32 {
        (2048 - i32::from(self.frequency)) * 16
    }

    fn write_control(&mut self, value: u16) {
        self.duty = Duty::from_bits(value.bits(6..8));
        self.length.write(value);
        self.envelope.write(value);
        if !self.envelope.dac_enabled() {
            self.enabled = false;
        }
    }

    fn read_control(&self) -> u16 {
        self.duty.bits() << 6 | self.envelope.read()
    }

    fn write_frequency(&mut self, value: u16, has_sweep: bool) {
        self.frequency = value.bits(0..11);
        self.length.enabled = value.bit(14);
        if value.bit(15) {
            self.enabled = self.envelope.dac_enabled();
            self.length.trigger();
            self.envelope.trigger();
            self.cycles = self.period();
            if has_sweep && self.sweep.trigger(self.frequency) {
                self.enabled = false;
            }
        }
    }

    fn read_frequency(&self) -> u16 {
        u16::from(self.length.enabled) << 14
    }

    fn advance(&mut self, cycles: i32) {
        self.cycles -= cycles;
        while self.cycles <= 0 {
            self.cycles += self.period();
            self.phase = (self.phase + 1) % 8;
        }
    }

    fn sample(&self) -> u8 {
        if self.enabled && self.duty.pattern().bit(u32::from(self.phase)) {
            self.envelope.volume
        } else {
            0
        }
    }

    fn tick_sweep(&mut self) {
        match self.sweep.tick() {
            SweepStep::Frequency(frequency) => self.frequency = frequency,
            SweepStep::Overflow => self.enabled = false,
            SweepStep::Idle => {}
        }
    }

    fn save_state(&self, writer: &mut Writer) {
        self.sweep.save_state(writer);
        writer.u8(self.duty.bits() as u8);
        self.length.save_state(writer);
        self.envelope.save_state(writer);
        writer.u16(self.frequency);
        writer.bool(self.enabled);
        writer.u8(self.phase);
        writer.i32(self.cycles);
    }

    fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.sweep.load_state(reader)?;
        self.duty = Duty::from_bits(u16::from(reader.u8()?));
        self.length.load_state(reader)?;
        self.envelope.load_state(reader)?;
        self.frequency = reader.u16()? & MAXIMUM_FREQUENCY;
        self.enabled = reader.bool()?;
        self.phase = reader.u8()? % 8;
        self.cycles = reader.i32()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum WaveVolume {
    #[default]
    Mute,
    Full,
    Half,
    Quarter,
}

impl WaveVolume {
    const ALL: [WaveVolume; 4] = [WaveVolume::Mute, WaveVolume::Full, WaveVolume::Half, WaveVolume::Quarter];

    fn from_bits(bits: u16) -> WaveVolume {
        WaveVolume::ALL[bits.bits(0..2) as usize]
    }

    fn bits(self) -> u16 {
        self as u16
    }

    fn apply(self, nibble: u8) -> u8 {
        match self {
            WaveVolume::Mute => 0,
            WaveVolume::Full => nibble,
            WaveVolume::Half => nibble / 2,
            WaveVolume::Quarter => nibble / 4,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Wave {
    two_banks: bool,
    playing_bank: usize,
    playback: bool,
    length: Length,
    volume: WaveVolume,
    force_three_quarters: bool,
    frequency: u16,
    enabled: bool,
    ram: [[u8; 16]; 2],
    position: u8,
    cycles: i32,
}

impl Wave {
    fn new() -> Wave {
        Wave {
            two_banks: false,
            playing_bank: 0,
            playback: false,
            length: Length::new(256),
            volume: WaveVolume::Mute,
            force_three_quarters: false,
            frequency: 0,
            enabled: false,
            ram: [[0; 16]; 2],
            position: 0,
            cycles: 0,
        }
    }

    fn period(&self) -> i32 {
        (2048 - i32::from(self.frequency)) * 8
    }

    fn write_bank(&mut self, value: u16) {
        self.two_banks = value.bit(5);
        self.playing_bank = usize::from(value.bit(6));
        self.playback = value.bit(7);
        if !self.playback {
            self.enabled = false;
        }
    }

    fn read_bank(&self) -> u16 {
        u16::from(self.two_banks) << 5 | (self.playing_bank as u16) << 6 | u16::from(self.playback) << 7
    }

    fn write_control(&mut self, value: u16) {
        self.length.write(value);
        self.volume = WaveVolume::from_bits(value.bits(13..15));
        self.force_three_quarters = value.bit(15);
    }

    fn read_control(&self) -> u16 {
        self.volume.bits() << 13 | u16::from(self.force_three_quarters) << 15
    }

    fn write_frequency(&mut self, value: u16) {
        self.frequency = value.bits(0..11);
        self.length.enabled = value.bit(14);
        if value.bit(15) {
            self.enabled = self.playback;
            self.length.trigger();
            self.position = 0;
            self.cycles = self.period();
        }
    }

    fn read_frequency(&self) -> u16 {
        u16::from(self.length.enabled) << 14
    }

    fn sample_count(&self) -> u8 {
        if self.two_banks {
            64
        } else {
            32
        }
    }

    fn advance(&mut self, cycles: i32) {
        self.cycles -= cycles;
        while self.cycles <= 0 {
            self.cycles += self.period();
            self.position = (self.position + 1) % self.sample_count();
        }
    }

    fn sample(&self) -> u8 {
        if !self.enabled {
            return 0;
        }
        let bank = (self.playing_bank + usize::from(self.position >= 32)) % 2;
        let byte = self.ram[bank][usize::from(self.position % 32) / 2];
        let nibble = if self.position % 2 == 0 { byte >> 4 } else { byte & 0xF };
        if self.force_three_quarters {
            nibble * 3 / 4
        } else {
            self.volume.apply(nibble)
        }
    }

    fn accessible_bank(&self) -> usize {
        (self.playing_bank + 1) % 2
    }

    fn read_ram(&self, offset: usize) -> u16 {
        let bank = &self.ram[self.accessible_bank()];
        u16::from_le_bytes([bank[offset], bank[offset + 1]])
    }

    fn write_ram(&mut self, offset: usize, value: u16) {
        let bank = self.accessible_bank();
        self.ram[bank][offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn save_state(&self, writer: &mut Writer) {
        writer.bool(self.two_banks);
        writer.u8(self.playing_bank as u8);
        writer.bool(self.playback);
        self.length.save_state(writer);
        writer.u8(self.volume.bits() as u8);
        writer.bool(self.force_three_quarters);
        writer.u16(self.frequency);
        writer.bool(self.enabled);
        writer.bytes(&self.ram[0]);
        writer.bytes(&self.ram[1]);
        writer.u8(self.position);
        writer.i32(self.cycles);
    }

    fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.two_banks = reader.bool()?;
        self.playing_bank = usize::from(reader.u8()?) % 2;
        self.playback = reader.bool()?;
        self.length.load_state(reader)?;
        self.volume = WaveVolume::from_bits(u16::from(reader.u8()?));
        self.force_three_quarters = reader.bool()?;
        self.frequency = reader.u16()? & MAXIMUM_FREQUENCY;
        self.enabled = reader.bool()?;
        reader.bytes_into(&mut self.ram[0])?;
        reader.bytes_into(&mut self.ram[1])?;
        self.position = reader.u8()? % 64;
        self.cycles = reader.i32()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Noise {
    length: Length,
    envelope: Envelope,
    divisor: u8,
    seven_bits: bool,
    shift: u8,
    enabled: bool,
    lfsr: u16,
    cycles: i32,
}

impl Noise {
    fn new() -> Noise {
        Noise {
            length: Length::new(64),
            lfsr: 0x7FFF,
            ..Default::default()
        }
    }

    fn period(&self) -> i32 {
        let divisor = if self.divisor == 0 { 8 } else { 16 * i32::from(self.divisor) };
        (divisor << (self.shift + 1)) * 4
    }

    fn write_control(&mut self, value: u16) {
        self.length.write(value);
        self.envelope.write(value);
        if !self.envelope.dac_enabled() {
            self.enabled = false;
        }
    }

    fn write_frequency(&mut self, value: u16) {
        self.divisor = value.bits(0..3) as u8;
        self.seven_bits = value.bit(3);
        self.shift = value.bits(4..8) as u8;
        self.length.enabled = value.bit(14);
        if value.bit(15) {
            self.enabled = self.envelope.dac_enabled();
            self.length.trigger();
            self.envelope.trigger();
            self.lfsr = 0x7FFF;
            self.cycles = self.period();
        }
    }

    fn read_frequency(&self) -> u16 {
        u16::from(self.divisor) | u16::from(self.seven_bits) << 3 | u16::from(self.shift) << 4 | u16::from(self.length.enabled) << 14
    }

    fn advance(&mut self, cycles: i32) {
        self.cycles -= cycles;
        while self.cycles <= 0 {
            self.cycles += self.period();
            let feedback = self.lfsr.bit(0) != self.lfsr.bit(1);
            self.lfsr = (self.lfsr >> 1).with_bit(14, feedback);
            if self.seven_bits {
                self.lfsr = self.lfsr.with_bit(6, feedback);
            }
        }
    }

    fn sample(&self) -> u8 {
        if self.enabled && !self.lfsr.bit(0) {
            self.envelope.volume
        } else {
            0
        }
    }

    fn save_state(&self, writer: &mut Writer) {
        self.length.save_state(writer);
        self.envelope.save_state(writer);
        writer.u8(self.divisor);
        writer.bool(self.seven_bits);
        writer.u8(self.shift);
        writer.bool(self.enabled);
        writer.u16(self.lfsr);
        writer.i32(self.cycles);
    }

    fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.length.load_state(reader)?;
        self.envelope.load_state(reader)?;
        self.divisor = reader.u8()?;
        self.seven_bits = reader.bool()?;
        self.shift = reader.u8()?;
        self.enabled = reader.bool()?;
        self.lfsr = reader.u16()?;
        self.cycles = reader.i32()?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct Fifo {
    samples: VecDeque<i8>,
    current: i8,
    level: f32,
    history: [i8; 4],
    last_pop: Option<u64>,
    interval: u64,
}

impl Fifo {
    fn push_word(&mut self, value: u32) {
        for byte in value.to_le_bytes() {
            if self.samples.len() < FIFO_CAPACITY {
                self.samples.push_back(byte as i8);
            }
        }
    }

    fn pop(&mut self, clock: u64, smooth: bool) -> bool {
        if let Some(sample) = self.samples.pop_front() {
            self.current = sample;
        }
        self.history.rotate_right(1);
        self.history[0] = self.current;
        self.interval = self.last_pop.map_or(0, |last_pop| clock - last_pop);
        self.last_pop = Some(clock);
        if !smooth {
            self.level = f32::from(self.current);
        }
        self.samples.len() <= FIFO_REFILL_THRESHOLD
    }

    fn interpolate(&self, clock: u64) -> f32 {
        let Some(last_pop) = self.last_pop.filter(|_| self.interval > 0) else {
            return f32::from(self.current);
        };
        let t = ((clock - last_pop) as f32 / self.interval as f32).min(1.0);
        let [p3, p2, p1, p0] = self.history.map(f32::from);
        let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
        let b = 2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3;
        let c = -p0 + p2;
        0.5 * (((a * t + b) * t + c) * t + 2.0 * p1)
    }

    fn restart(&mut self) {
        self.level = f32::from(self.current);
        self.history = [0; 4];
        self.last_pop = None;
        self.interval = 0;
    }

    fn reset(&mut self) {
        self.samples.clear();
        self.current = 0;
        self.restart();
    }

    fn save_state(&self, writer: &mut Writer) {
        let bytes: Vec<u8> = self.samples.iter().map(|sample| *sample as u8).collect();
        writer.sized_bytes(&bytes);
        writer.u8(self.current as u8);
    }

    fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.samples = reader.sized_bytes()?.iter().map(|byte| *byte as i8).collect();
        self.current = reader.u8()? as i8;
        self.restart();
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PsgScale {
    #[default]
    Quarter,
    Half,
    Full,
}

impl PsgScale {
    fn from_bits(bits: u16) -> PsgScale {
        match bits.bits(0..2) {
            0 => PsgScale::Quarter,
            1 => PsgScale::Half,
            _ => PsgScale::Full,
        }
    }

    fn bits(self) -> u16 {
        self as u16
    }

    fn factor(self) -> f32 {
        match self {
            PsgScale::Quarter => 0.25,
            PsgScale::Half => 0.5,
            PsgScale::Full => 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Output {
    psg_volume: u8,
    psg_channels: u8,
    fifo: [bool; 2],
}

pub struct Apu {
    square1: Square,
    square2: Square,
    wave: Wave,
    noise: Noise,
    fifo: [Fifo; 2],
    outputs: [Output; 2],
    psg_scale: PsgScale,
    fifo_full_volume: [bool; 2],
    fifo_timer: [u8; 2],
    master_enable: bool,
    bias: u16,
    frame_sequencer_cycles: u32,
    frame_sequencer_step: u8,
    clock: u64,
    frame_cycles: u32,
    grid_cycles: u32,
    smooth: bool,
    last_level: [f32; 2],
    synth: [Synth; 2],
}

impl Apu {
    pub fn new() -> Apu {
        Apu {
            square1: Square::new(),
            square2: Square::new(),
            wave: Wave::new(),
            noise: Noise::new(),
            fifo: [Fifo::default(), Fifo::default()],
            outputs: [Output::default(); 2],
            psg_scale: PsgScale::default(),
            fifo_full_volume: [false; 2],
            fifo_timer: [0; 2],
            master_enable: false,
            bias: DEFAULT_BIAS,
            frame_sequencer_cycles: 0,
            frame_sequencer_step: 0,
            clock: 0,
            frame_cycles: 0,
            grid_cycles: 0,
            smooth: false,
            last_level: [0.0; 2],
            synth: [Synth::new(SAMPLE_RATE), Synth::new(SAMPLE_RATE)],
        }
    }

    pub fn read_u16(&self, offset: u32) -> u16 {
        let [left, right] = self.outputs;
        match offset {
            0x60 => self.square1.sweep.read(),
            0x62 => self.square1.read_control(),
            0x64 => self.square1.read_frequency(),
            0x68 => self.square2.read_control(),
            0x6C => self.square2.read_frequency(),
            0x70 => self.wave.read_bank(),
            0x72 => self.wave.read_control(),
            0x74 => self.wave.read_frequency(),
            0x78 => self.noise.envelope.read(),
            0x7C => self.noise.read_frequency(),
            0x80 => u16::from(right.psg_volume) | u16::from(left.psg_volume) << 4 | u16::from(right.psg_channels) << 8 | u16::from(left.psg_channels) << 12,
            0x82 => {
                self.psg_scale.bits()
                    | u16::from(self.fifo_full_volume[0]) << 2
                    | u16::from(self.fifo_full_volume[1]) << 3
                    | u16::from(right.fifo[0]) << 8
                    | u16::from(left.fifo[0]) << 9
                    | u16::from(self.fifo_timer[0]) << 10
                    | u16::from(right.fifo[1]) << 12
                    | u16::from(left.fifo[1]) << 13
                    | u16::from(self.fifo_timer[1]) << 14
            }
            0x84 => {
                u16::from(self.square1.enabled) | u16::from(self.square2.enabled) << 1 | u16::from(self.wave.enabled) << 2 | u16::from(self.noise.enabled) << 3 | u16::from(self.master_enable) << 7
            }
            0x88 => self.bias,
            0x90..=0x9F => self.wave.read_ram((offset - 0x90) as usize),
            _ => 0,
        }
    }

    pub fn write_u16(&mut self, offset: u32, value: u16) {
        if !self.master_enable && (0x60..0x84).contains(&offset) {
            return;
        }
        self.write_register(offset, value);
        self.emit();
    }

    fn write_register(&mut self, offset: u32, value: u16) {
        match offset {
            0x60 => self.square1.sweep.write(value),
            0x62 => self.square1.write_control(value),
            0x64 => self.square1.write_frequency(value, true),
            0x68 => self.square2.write_control(value),
            0x6C => self.square2.write_frequency(value, false),
            0x70 => self.wave.write_bank(value),
            0x72 => self.wave.write_control(value),
            0x74 => self.wave.write_frequency(value),
            0x78 => self.noise.write_control(value),
            0x7C => self.noise.write_frequency(value),
            0x80 => {
                let [left, right] = &mut self.outputs;
                right.psg_volume = value.bits(0..3) as u8;
                left.psg_volume = value.bits(4..7) as u8;
                right.psg_channels = value.bits(8..12) as u8;
                left.psg_channels = value.bits(12..16) as u8;
            }
            0x82 => {
                let [left, right] = &mut self.outputs;
                self.psg_scale = PsgScale::from_bits(value);
                self.fifo_full_volume = [value.bit(2), value.bit(3)];
                right.fifo = [value.bit(8), value.bit(12)];
                left.fifo = [value.bit(9), value.bit(13)];
                self.fifo_timer = [u8::from(value.bit(10)), u8::from(value.bit(14))];
                for (fifo, reset) in self.fifo.iter_mut().zip([value.bit(11), value.bit(15)]) {
                    if reset {
                        fifo.reset();
                    }
                }
            }
            0x84 => {
                let enable = value.bit(7);
                if !enable && self.master_enable {
                    self.square1 = Square::new();
                    self.square2 = Square::new();
                    self.wave = Wave::new();
                    self.noise = Noise::new();
                    for output in &mut self.outputs {
                        output.psg_volume = 0;
                        output.psg_channels = 0;
                    }
                }
                self.master_enable = enable;
            }
            0x88 => self.bias = value & 0xC3FE,
            0x90..=0x9F => self.wave.write_ram((offset - 0x90) as usize, value),
            0xA0 | 0xA2 => self.fifo[0].push_word(u32::from(value) * 0x0001_0001),
            0xA4 | 0xA6 => self.fifo[1].push_word(u32::from(value) * 0x0001_0001),
            _ => {}
        }
    }

    pub fn write_fifo(&mut self, fifo: usize, value: u32) {
        self.fifo[fifo].push_word(value);
    }

    pub fn timer_overflow(&mut self, timer: u8) -> [bool; 2] {
        let (clock, smooth, timers) = (self.clock, self.smooth, self.fifo_timer);
        let fifos = &mut self.fifo;
        let refill = std::array::from_fn(|index| timers[index] == timer && fifos[index].pop(clock, smooth));
        self.emit();
        refill
    }

    pub fn run(&mut self, cycles: u32) {
        self.frame_sequencer_cycles += cycles;
        while self.frame_sequencer_cycles >= FRAME_SEQUENCER_CYCLES {
            self.frame_sequencer_cycles -= FRAME_SEQUENCER_CYCLES;
            self.tick_frame_sequencer();
        }
        let mut remaining = cycles;
        while remaining > 0 {
            let step = remaining.min(GRID_CYCLES - self.grid_cycles);
            self.grid_cycles += step;
            self.frame_cycles += step;
            self.clock += u64::from(step);
            remaining -= step;
            if self.grid_cycles == GRID_CYCLES {
                self.grid_cycles = 0;
                self.advance_channels();
                if self.smooth {
                    for fifo in &mut self.fifo {
                        fifo.level = fifo.interpolate(self.clock);
                    }
                }
                self.emit();
            }
        }
        if self.frame_cycles >= FLUSH_CYCLES {
            self.flush_synth();
        }
    }

    fn advance_channels(&mut self) {
        let cycles = GRID_CYCLES as i32;
        self.square1.advance(cycles);
        self.square2.advance(cycles);
        self.wave.advance(cycles);
        self.noise.advance(cycles);
    }

    fn emit(&mut self) {
        let levels = self.levels();
        let frame_cycles = self.frame_cycles;
        for ((synth, last), level) in self.synth.iter_mut().zip(&mut self.last_level).zip(levels) {
            if level != *last {
                synth.add_delta(frame_cycles, level - *last);
                *last = level;
            }
        }
    }

    fn flush_synth(&mut self) {
        for synth in &mut self.synth {
            synth.end_frame(self.frame_cycles);
        }
        self.frame_cycles = 0;
    }

    fn tick_frame_sequencer(&mut self) {
        let step = self.frame_sequencer_step;
        self.frame_sequencer_step = (step + 1) % 8;
        if step % 2 == 0 {
            if self.square1.length.tick() {
                self.square1.enabled = false;
            }
            if self.square2.length.tick() {
                self.square2.enabled = false;
            }
            if self.wave.length.tick() {
                self.wave.enabled = false;
            }
            if self.noise.length.tick() {
                self.noise.enabled = false;
            }
        }
        if step == 2 || step == 6 {
            self.square1.tick_sweep();
        }
        if step == 7 {
            self.square1.envelope.tick();
            self.square2.envelope.tick();
            self.noise.envelope.tick();
        }
    }

    fn psg_samples(&self) -> [u8; 4] {
        [self.square1.sample(), self.square2.sample(), self.wave.sample(), self.noise.sample()]
    }

    fn level(&self, side: Side) -> f32 {
        if !self.master_enable {
            return 0.0;
        }
        let output = self.outputs[side as usize];
        let psg: f32 = self
            .psg_samples()
            .iter()
            .enumerate()
            .filter(|(channel, _)| output.psg_channels.bit(*channel as u32))
            .map(|(_, sample)| f32::from(*sample))
            .sum();
        let psg_level = psg * f32::from(output.psg_volume) * PSG_FULL_SCALE * self.psg_scale.factor();
        let fifo_level: f32 = self
            .fifo
            .iter()
            .zip(output.fifo)
            .zip(self.fifo_full_volume)
            .filter(|((_, enabled), _)| *enabled)
            .map(|((fifo, _), full_volume)| if full_volume { fifo.level * 2.0 } else { fifo.level })
            .sum();
        let bias = f32::from(self.bias & 0x3FE);
        (bias + psg_level + fifo_level).clamp(0.0, DAC_MAXIMUM) - DAC_CENTER
    }

    fn levels(&self) -> [f32; 2] {
        Side::ALL.map(|side| self.level(side))
    }

    pub fn save_state(&self, writer: &mut Writer) {
        self.square1.save_state(writer);
        self.square2.save_state(writer);
        self.wave.save_state(writer);
        self.noise.save_state(writer);
        for fifo in &self.fifo {
            fifo.save_state(writer);
        }
        let [left, right] = self.outputs;
        writer.u8(right.psg_volume);
        writer.u8(left.psg_volume);
        writer.u8(right.psg_channels);
        writer.u8(left.psg_channels);
        writer.u8(self.psg_scale.bits() as u8);
        writer.bools(&self.fifo_full_volume);
        writer.bools(&right.fifo);
        writer.bools(&left.fifo);
        writer.bytes(&self.fifo_timer);
        writer.bool(self.master_enable);
        writer.u16(self.bias);
        writer.u32(self.frame_sequencer_cycles);
        writer.u8(self.frame_sequencer_step);
        writer.u64(self.synth[0].phase(self.frame_cycles).to_bits());
    }

    pub fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.square1.load_state(reader)?;
        self.square2.load_state(reader)?;
        self.wave.load_state(reader)?;
        self.noise.load_state(reader)?;
        for fifo in &mut self.fifo {
            fifo.load_state(reader)?;
        }
        let [left, right] = &mut self.outputs;
        right.psg_volume = reader.u8()?;
        left.psg_volume = reader.u8()?;
        right.psg_channels = reader.u8()?;
        left.psg_channels = reader.u8()?;
        self.psg_scale = PsgScale::from_bits(u16::from(reader.u8()?));
        reader.bools(&mut self.fifo_full_volume)?;
        reader.bools(&mut right.fifo)?;
        reader.bools(&mut left.fifo)?;
        reader.bytes_into(&mut self.fifo_timer)?;
        self.master_enable = reader.bool()?;
        self.bias = reader.u16()?;
        self.frame_sequencer_cycles = reader.u32()?;
        self.frame_sequencer_step = reader.u8()? % 8;
        let phase = f64::from_bits(reader.u64()?);
        for synth in &mut self.synth {
            synth.set_phase(phase);
        }
        self.clock = 0;
        self.frame_cycles = 0;
        self.grid_cycles = 0;
        self.last_level = self.levels();
        Ok(())
    }

    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        for synth in &mut self.synth {
            synth.set_sample_rate(sample_rate);
        }
        self.frame_cycles = 0;
    }

    pub fn set_smooth(&mut self, smooth: bool) {
        self.smooth = smooth;
        for fifo in &mut self.fifo {
            fifo.level = f32::from(fifo.current);
        }
    }

    pub fn take_samples(&mut self) -> Vec<i16> {
        self.flush_synth();
        let [left, right] = &mut self.synth;
        left.take().into_iter().zip(right.take()).flat_map(|(left, right)| [left, right]).collect()
    }

    pub fn pending_samples(&self) -> usize {
        self.synth[0].available(self.frame_cycles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_apu() -> Apu {
        let mut apu = Apu::new();
        apu.write_u16(0x84, 0x80);
        apu.write_u16(0x80, 0xFF77);
        apu.write_u16(0x82, 0x2);
        apu
    }

    fn run_samples(apu: &mut Apu, count: usize) -> Vec<i16> {
        while apu.pending_samples() < count {
            apu.run(64);
        }
        apu.take_samples()
    }

    fn stream_fifo(apu: &mut Apu, interval: u32, samples: impl Iterator<Item = i8>) {
        for sample in samples {
            apu.fifo[0].samples.push_back(sample);
            apu.run(interval);
            apu.timer_overflow(0);
        }
    }

    #[test]
    fn test_square_wave_toggles_at_programmed_frequency() {
        let mut apu = enabled_apu();
        apu.write_u16(0x62, 0xF000 | 2 << 6);
        apu.write_u16(0x64, 0x8000 | 1792);
        let samples = run_samples(&mut apu, 8192);
        let left: Vec<i16> = samples.iter().step_by(2).skip(2048).copied().collect();
        let high = *left.iter().max().unwrap();
        let low = *left.iter().min().unwrap();
        assert!(high > 0 && low < 0, "high {} low {}", high, low);
        let crossings = left.windows(2).filter(|pair| (pair[0] < 0) != (pair[1] < 0)).count();
        let expected_period_samples = f64::from(SAMPLE_RATE) / 512.0;
        let expected_crossings = (left.len() as f64 / expected_period_samples * 2.0) as usize;
        assert!((crossings as i64 - expected_crossings as i64).abs() <= 2, "{} crossings, expected {}", crossings, expected_crossings);
    }

    #[test]
    fn test_smooth_direct_sound_rounds_off_steps() {
        let rise = |smooth: bool| {
            let mut apu = enabled_apu();
            apu.set_smooth(smooth);
            apu.write_u16(0x82, 0x2 | 0x4 | 0x100 | 0x200);
            stream_fifo(&mut apu, 4096, std::iter::repeat(0).take(40).chain(std::iter::repeat(0x40).take(40)));
            let left: Vec<i32> = apu.take_samples().iter().step_by(2).map(|sample| i32::from(*sample)).collect();
            let edge = left.iter().position(|sample| *sample > left.iter().max().unwrap() / 2).unwrap();
            let level = left[edge + 20];
            let rising = left[edge - 20..edge + 20].iter().filter(|sample| **sample > level / 8 && **sample < level * 7 / 8).count();
            (level, rising)
        };
        let (exact_level, exact_rising) = rise(false);
        let (smooth_level, smooth_rising) = rise(true);
        assert!((exact_level - smooth_level).abs() < exact_level / 8, "levels {} and {}", exact_level, smooth_level);
        assert!(exact_rising <= 2, "{} samples rising in exact mode", exact_rising);
        assert!(smooth_rising >= 4, "{} samples rising in smooth mode", smooth_rising);
    }

    #[test]
    fn test_master_disable_is_silent() {
        let mut apu = Apu::new();
        apu.write_u16(0x62, 0xF000 | 2 << 6);
        apu.write_u16(0x64, 0x8000 | 1792);
        assert!(run_samples(&mut apu, 256).iter().all(|sample| *sample == 0));
    }

    #[test]
    fn test_bias_resolution_bits_do_not_offset_output() {
        let mut apu = enabled_apu();
        apu.write_u16(0x88, 0xC200);
        assert_eq!(apu.read_u16(0x88), 0xC200);
        assert_eq!(apu.levels(), [0.0, 0.0]);
        apu.write_u16(0x82, 0x2 | 0x4 | 0x100 | 0x200);
        apu.write_fifo(0, 0x0000_0060);
        apu.timer_overflow(0);
        assert_eq!(apu.levels(), [192.0, 192.0]);
    }

    #[test]
    fn test_length_counter_stops_channel() {
        let mut apu = enabled_apu();
        apu.write_u16(0x68, 0xF000 | 2 << 6 | 62);
        apu.write_u16(0x6C, 0x8000 | 0x4000 | 1792);
        assert_eq!(apu.read_u16(0x84) & 0b10, 0b10);
        apu.run(FRAME_SEQUENCER_CYCLES * 2);
        assert_eq!(apu.read_u16(0x84) & 0b10, 0b10);
        apu.run(FRAME_SEQUENCER_CYCLES * 3);
        assert_eq!(apu.read_u16(0x84) & 0b10, 0);
    }

    #[test]
    fn test_envelope_decreases_volume() {
        let mut apu = enabled_apu();
        apu.write_u16(0x62, 0xF000 | 1 << 8 | 2 << 6);
        apu.write_u16(0x64, 0x8000 | 1792);
        assert_eq!(apu.square1.envelope.volume, 15);
        apu.run(FRAME_SEQUENCER_CYCLES * 8);
        assert_eq!(apu.square1.envelope.volume, 14);
        apu.run(FRAME_SEQUENCER_CYCLES * 8 * 14);
        assert_eq!(apu.square1.envelope.volume, 0);
    }

    #[test]
    fn test_wave_ram_writes_go_to_the_other_bank() {
        let mut apu = enabled_apu();
        apu.write_u16(0x70, 0x40);
        apu.write_u16(0x90, 0x1234);
        assert_eq!(apu.wave.ram[0][0], 0x34);
        assert_eq!(apu.read_u16(0x90), 0x1234);
        apu.write_u16(0x70, 0x00);
        assert_eq!(apu.read_u16(0x90), 0);
    }

    #[test]
    fn test_fifo_outputs_samples_on_timer_overflow() {
        let mut apu = enabled_apu();
        apu.write_u16(0x82, 0x2 | 0x4 | 0x100 | 0x200);
        apu.write_fifo(0, 0x0000_0060);
        for _ in 0..7 {
            apu.write_fifo(0, 0);
        }
        assert!(!apu.timer_overflow(0)[0]);
        assert_eq!(apu.levels(), [192.0, 192.0]);
        for _ in 0..14 {
            assert!(!apu.timer_overflow(0)[0]);
        }
        assert!(apu.timer_overflow(0)[0]);
        assert_eq!(apu.levels(), [0.0, 0.0]);
    }

    #[test]
    fn test_noise_produces_varying_output() {
        let mut apu = enabled_apu();
        apu.write_u16(0x78, 0xF000);
        apu.write_u16(0x7C, 0x8000 | 0x10);
        let samples = run_samples(&mut apu, 2048);
        assert!(samples.iter().any(|sample| *sample != samples[0]));
    }

    #[test]
    fn test_sweep_raises_the_frequency_until_it_overflows() {
        let mut apu = enabled_apu();
        apu.write_u16(0x60, 1 << 4 | 2);
        apu.write_u16(0x62, 0xF000 | 2 << 6);
        apu.write_u16(0x64, 0x8000 | 256);
        apu.run(FRAME_SEQUENCER_CYCLES * 3);
        assert_eq!(apu.square1.frequency, 320);
        assert!(apu.square1.enabled);
        apu.run(FRAME_SEQUENCER_CYCLES * 32);
        assert_eq!(apu.square1.frequency, 1525);
        assert!(!apu.square1.enabled);
    }
}
