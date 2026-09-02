use std::collections::VecDeque;

use super::cpu::CPU_FREQUENCY;

pub const SAMPLE_RATE: u32 = 48_000;
const FRAME_SEQUENCER_CYCLES: u32 = 32_768;
const FIFO_CAPACITY: usize = 32;
const FIFO_REFILL_THRESHOLD: usize = 16;
const DUTY_PATTERNS: [u8; 4] = [0b0000_0001, 0b0000_0011, 0b0000_1111, 0b1111_1100];
const DEFAULT_BIAS: u16 = 0x200;

pub const FIFO_A: u32 = 0x0400_00A0;
pub const FIFO_B: u32 = 0x0400_00A4;

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
        self.initial_volume = (value >> 12) as u8;
        self.increase = value & 0x0800 != 0;
        self.step_time = (value >> 8 & 0b111) as u8;
    }

    fn read(&self) -> u16 {
        (self.initial_volume as u16) << 12 | (self.increase as u16) << 11 | (self.step_time as u16) << 8
    }

    fn trigger(&mut self) {
        self.volume = self.initial_volume;
        self.counter = self.step_time;
    }

    fn dac_enabled(&self) -> bool {
        self.initial_volume != 0 || self.increase
    }

    fn tick(&mut self) {
        if self.step_time != 0 {
            self.counter = self.counter.saturating_sub(1);
            if self.counter == 0 {
                self.counter = self.step_time;
                if self.increase && self.volume < 15 {
                    self.volume += 1;
                } else if !self.increase && self.volume > 0 {
                    self.volume -= 1;
                }
            }
        }
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
        Length {
            enabled: false,
            counter: 0,
            maximum,
        }
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
        self.shift = (value & 0b111) as u8;
        self.decrease = value & 0b1000 != 0;
        self.time = (value >> 4 & 0b111) as u8;
    }

    fn read(&self) -> u16 {
        self.shift as u16 | (self.decrease as u16) << 3 | (self.time as u16) << 4
    }

    fn trigger(&mut self, frequency: u16) -> bool {
        self.shadow_frequency = frequency;
        self.counter = if self.time == 0 { 8 } else { self.time };
        self.enabled = self.time != 0 || self.shift != 0;
        self.shift != 0 && self.next_frequency() > 2047
    }

    fn next_frequency(&self) -> u16 {
        let delta = self.shadow_frequency >> self.shift;
        if self.decrease {
            self.shadow_frequency.wrapping_sub(delta)
        } else {
            self.shadow_frequency + delta
        }
    }

    fn tick(&mut self) -> Option<Result<u16, ()>> {
        self.counter = self.counter.saturating_sub(1);
        if self.counter == 0 {
            self.counter = if self.time == 0 { 8 } else { self.time };
            if self.enabled && self.time != 0 {
                let next = self.next_frequency();
                if next > 2047 {
                    return Some(Err(()));
                }
                if self.shift != 0 {
                    self.shadow_frequency = next;
                    if self.next_frequency() > 2047 {
                        return Some(Err(()));
                    }
                    return Some(Ok(next));
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Square {
    sweep: Sweep,
    duty: u8,
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
        (2048 - self.frequency as i32) * 16
    }

    fn write_control(&mut self, value: u16) {
        self.duty = (value >> 6 & 0b11) as u8;
        self.length.write(value);
        self.envelope.write(value);
        if !self.envelope.dac_enabled() {
            self.enabled = false;
        }
    }

    fn read_control(&self) -> u16 {
        (self.duty as u16) << 6 | self.envelope.read()
    }

    fn write_frequency(&mut self, value: u16, has_sweep: bool) {
        self.frequency = value & 0x7FF;
        self.length.enabled = value & 0x4000 != 0;
        if value & 0x8000 != 0 {
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
        (self.length.enabled as u16) << 14
    }

    fn advance(&mut self, cycles: i32) {
        self.cycles -= cycles;
        while self.cycles <= 0 {
            self.cycles += self.period();
            self.phase = (self.phase + 1) & 7;
        }
    }

    fn sample(&self) -> u8 {
        if self.enabled && DUTY_PATTERNS[self.duty as usize] >> self.phase & 1 != 0 {
            self.envelope.volume
        } else {
            0
        }
    }

    fn tick_sweep(&mut self) {
        match self.sweep.tick() {
            Some(Ok(frequency)) => self.frequency = frequency,
            Some(Err(())) => self.enabled = false,
            None => {}
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Wave {
    two_banks: bool,
    playing_bank: usize,
    playback: bool,
    length: Length,
    volume: u8,
    force_75: bool,
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
            volume: 0,
            force_75: false,
            frequency: 0,
            enabled: false,
            ram: [[0; 16]; 2],
            position: 0,
            cycles: 0,
        }
    }

    fn period(&self) -> i32 {
        (2048 - self.frequency as i32) * 8
    }

    fn write_bank(&mut self, value: u16) {
        self.two_banks = value & 0x20 != 0;
        self.playing_bank = (value >> 6 & 1) as usize;
        self.playback = value & 0x80 != 0;
        if !self.playback {
            self.enabled = false;
        }
    }

    fn read_bank(&self) -> u16 {
        (self.two_banks as u16) << 5 | (self.playing_bank as u16) << 6 | (self.playback as u16) << 7
    }

    fn write_control(&mut self, value: u16) {
        self.length.write(value);
        self.volume = (value >> 13 & 0b11) as u8;
        self.force_75 = value & 0x8000 != 0;
    }

    fn read_control(&self) -> u16 {
        (self.volume as u16) << 13 | (self.force_75 as u16) << 15
    }

    fn write_frequency(&mut self, value: u16) {
        self.frequency = value & 0x7FF;
        self.length.enabled = value & 0x4000 != 0;
        if value & 0x8000 != 0 {
            self.enabled = self.playback;
            self.length.trigger();
            self.position = 0;
            self.cycles = self.period();
        }
    }

    fn read_frequency(&self) -> u16 {
        (self.length.enabled as u16) << 14
    }

    fn advance(&mut self, cycles: i32) {
        self.cycles -= cycles;
        while self.cycles <= 0 {
            self.cycles += self.period();
            self.position = (self.position + 1) % if self.two_banks { 64 } else { 32 };
        }
    }

    fn sample(&self) -> u8 {
        if !self.enabled {
            return 0;
        }
        let bank = (self.playing_bank + (self.position >= 32) as usize) % 2;
        let byte = self.ram[bank][(self.position as usize % 32) / 2];
        let nibble = if self.position % 2 == 0 { byte >> 4 } else { byte & 0xF };
        if self.force_75 {
            nibble * 3 / 4
        } else {
            match self.volume {
                0 => 0,
                1 => nibble,
                2 => nibble / 2,
                _ => nibble / 4,
            }
        }
    }

    fn accessible_bank(&self) -> usize {
        (self.playing_bank + 1) % 2
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
        let divisor = if self.divisor == 0 { 8 } else { 16 * self.divisor as i32 };
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
        self.divisor = (value & 0b111) as u8;
        self.seven_bits = value & 0b1000 != 0;
        self.shift = (value >> 4 & 0xF) as u8;
        self.length.enabled = value & 0x4000 != 0;
        if value & 0x8000 != 0 {
            self.enabled = self.envelope.dac_enabled();
            self.length.trigger();
            self.envelope.trigger();
            self.lfsr = 0x7FFF;
            self.cycles = self.period();
        }
    }

    fn read_frequency(&self) -> u16 {
        self.divisor as u16 | (self.seven_bits as u16) << 3 | (self.shift as u16) << 4 | (self.length.enabled as u16) << 14
    }

    fn advance(&mut self, cycles: i32) {
        self.cycles -= cycles;
        while self.cycles <= 0 {
            self.cycles += self.period();
            let feedback = (self.lfsr ^ self.lfsr >> 1) & 1;
            self.lfsr = self.lfsr >> 1 | feedback << 14;
            if self.seven_bits {
                self.lfsr = self.lfsr & !(1 << 6) | feedback << 6;
            }
        }
    }

    fn sample(&self) -> u8 {
        if self.enabled && self.lfsr & 1 == 0 {
            self.envelope.volume
        } else {
            0
        }
    }
}

#[derive(Debug, Default)]
struct Fifo {
    samples: VecDeque<i8>,
    current: i8,
}

impl Fifo {
    fn push_word(&mut self, value: u32) {
        for byte in value.to_le_bytes() {
            if self.samples.len() < FIFO_CAPACITY {
                self.samples.push_back(byte as i8);
            }
        }
    }

    fn pop(&mut self) -> bool {
        if let Some(sample) = self.samples.pop_front() {
            self.current = sample;
        }
        self.samples.len() <= FIFO_REFILL_THRESHOLD
    }

    fn reset(&mut self) {
        self.samples.clear();
        self.current = 0;
    }
}

pub struct Apu {
    square1: Square,
    square2: Square,
    wave: Wave,
    noise: Noise,
    fifo: [Fifo; 2],
    psg_volume_right: u8,
    psg_volume_left: u8,
    psg_enable_right: u8,
    psg_enable_left: u8,
    psg_scale: u8,
    fifo_full_volume: [bool; 2],
    fifo_enable_right: [bool; 2],
    fifo_enable_left: [bool; 2],
    fifo_timer: [u8; 2],
    master_enable: bool,
    bias: u16,
    frame_sequencer_cycles: u32,
    frame_sequencer_step: u8,
    sample_cycles: u64,
    channel_cycles: i32,
    samples: Vec<i16>,
}

impl Apu {
    pub fn new() -> Apu {
        Apu {
            square1: Square::new(),
            square2: Square::new(),
            wave: Wave::new(),
            noise: Noise::new(),
            fifo: [Fifo::default(), Fifo::default()],
            psg_volume_right: 0,
            psg_volume_left: 0,
            psg_enable_right: 0,
            psg_enable_left: 0,
            psg_scale: 0,
            fifo_full_volume: [false; 2],
            fifo_enable_right: [false; 2],
            fifo_enable_left: [false; 2],
            fifo_timer: [0; 2],
            master_enable: false,
            bias: DEFAULT_BIAS,
            frame_sequencer_cycles: 0,
            frame_sequencer_step: 0,
            sample_cycles: 0,
            channel_cycles: 0,
            samples: Vec::new(),
        }
    }

    pub fn read_u16(&self, offset: u32) -> u16 {
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
            0x80 => {
                self.psg_volume_right as u16
                    | (self.psg_volume_left as u16) << 4
                    | (self.psg_enable_right as u16) << 8
                    | (self.psg_enable_left as u16) << 12
            }
            0x82 => {
                self.psg_scale as u16
                    | (self.fifo_full_volume[0] as u16) << 2
                    | (self.fifo_full_volume[1] as u16) << 3
                    | (self.fifo_enable_right[0] as u16) << 8
                    | (self.fifo_enable_left[0] as u16) << 9
                    | (self.fifo_timer[0] as u16) << 10
                    | (self.fifo_enable_right[1] as u16) << 12
                    | (self.fifo_enable_left[1] as u16) << 13
                    | (self.fifo_timer[1] as u16) << 14
            }
            0x84 => {
                self.square1.enabled as u16
                    | (self.square2.enabled as u16) << 1
                    | (self.wave.enabled as u16) << 2
                    | (self.noise.enabled as u16) << 3
                    | (self.master_enable as u16) << 7
            }
            0x88 => self.bias,
            0x90..=0x9F => {
                let bank = &self.wave.ram[self.wave.accessible_bank()];
                let index = (offset - 0x90) as usize;
                bank[index] as u16 | (bank[index + 1] as u16) << 8
            }
            _ => 0,
        }
    }

    pub fn write_u16(&mut self, offset: u32, value: u16) {
        if !self.master_enable && (0x60..0x84).contains(&offset) {
            return;
        }
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
                self.psg_volume_right = (value & 0b111) as u8;
                self.psg_volume_left = (value >> 4 & 0b111) as u8;
                self.psg_enable_right = (value >> 8 & 0xF) as u8;
                self.psg_enable_left = (value >> 12 & 0xF) as u8;
            }
            0x82 => {
                self.psg_scale = (value & 0b11) as u8;
                self.fifo_full_volume = [value & 0x4 != 0, value & 0x8 != 0];
                self.fifo_enable_right = [value & 0x100 != 0, value & 0x1000 != 0];
                self.fifo_enable_left = [value & 0x200 != 0, value & 0x2000 != 0];
                self.fifo_timer = [(value >> 10 & 1) as u8, (value >> 14 & 1) as u8];
                if value & 0x800 != 0 {
                    self.fifo[0].reset();
                }
                if value & 0x8000 != 0 {
                    self.fifo[1].reset();
                }
            }
            0x84 => {
                let enable = value & 0x80 != 0;
                if !enable && self.master_enable {
                    self.square1 = Square::new();
                    self.square2 = Square::new();
                    self.wave = Wave::new();
                    self.noise = Noise::new();
                    self.psg_volume_right = 0;
                    self.psg_volume_left = 0;
                    self.psg_enable_right = 0;
                    self.psg_enable_left = 0;
                }
                self.master_enable = enable;
            }
            0x88 => self.bias = value & 0xC3FE,
            0x90..=0x9F => {
                let bank = self.wave.accessible_bank();
                let index = (offset - 0x90) as usize;
                self.wave.ram[bank][index] = value as u8;
                self.wave.ram[bank][index + 1] = (value >> 8) as u8;
            }
            0xA0 | 0xA2 => self.fifo[0].push_word(value as u32 | (value as u32) << 16),
            0xA4 | 0xA6 => self.fifo[1].push_word(value as u32 | (value as u32) << 16),
            _ => {}
        }
    }

    pub fn write_fifo(&mut self, fifo: usize, value: u32) {
        self.fifo[fifo].push_word(value);
    }

    pub fn timer_overflow(&mut self, timer: u8) -> [bool; 2] {
        let mut refill = [false; 2];
        for fifo in 0..2 {
            if self.fifo_timer[fifo] == timer {
                refill[fifo] = self.fifo[fifo].pop();
            }
        }
        refill
    }

    pub fn run(&mut self, cycles: u32) {
        self.sample_cycles += cycles as u64 * SAMPLE_RATE as u64;
        self.channel_cycles += cycles as i32;
        self.frame_sequencer_cycles += cycles;
        while self.frame_sequencer_cycles >= FRAME_SEQUENCER_CYCLES {
            self.frame_sequencer_cycles -= FRAME_SEQUENCER_CYCLES;
            self.tick_frame_sequencer();
        }
        while self.sample_cycles >= CPU_FREQUENCY {
            self.sample_cycles -= CPU_FREQUENCY;
            self.advance_channels();
            let (left, right) = self.mix();
            self.samples.push(left);
            self.samples.push(right);
        }
    }

    fn advance_channels(&mut self) {
        let cycles = self.channel_cycles;
        self.channel_cycles = 0;
        self.square1.advance(cycles);
        self.square2.advance(cycles);
        self.wave.advance(cycles);
        self.noise.advance(cycles);
    }

    fn tick_frame_sequencer(&mut self) {
        let step = self.frame_sequencer_step;
        self.frame_sequencer_step = (step + 1) & 7;
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

    fn mix(&self) -> (i16, i16) {
        if !self.master_enable {
            return (0, 0);
        }
        let psg = [self.square1.sample(), self.square2.sample(), self.wave.sample(), self.noise.sample()];
        let side = |volume: u8, enable: u8, fifo_right_or_left: [bool; 2]| {
            let mut psg_sum = 0i32;
            for (channel, sample) in psg.iter().enumerate() {
                if enable & (1 << channel) != 0 {
                    psg_sum += *sample as i32;
                }
            }
            let psg_level = (psg_sum * volume as i32 * 4 / 7) >> (2 - self.psg_scale.min(2));
            let mut level = self.bias as i32 + psg_level;
            for fifo in 0..2 {
                if fifo_right_or_left[fifo] {
                    let sample = self.fifo[fifo].current as i32;
                    level += if self.fifo_full_volume[fifo] { sample * 2 } else { sample };
                }
            }
            ((level.clamp(0, 0x3FF) - 0x200) << 6) as i16
        };
        (
            side(self.psg_volume_left, self.psg_enable_left, self.fifo_enable_left),
            side(self.psg_volume_right, self.psg_enable_right, self.fifo_enable_right),
        )
    }

    pub fn take_samples(&mut self) -> Vec<i16> {
        std::mem::take(&mut self.samples)
    }

    pub fn pending_samples(&self) -> usize {
        self.samples.len() / 2
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

    #[test]
    fn test_square_wave_toggles_at_programmed_frequency() {
        let mut apu = enabled_apu();
        apu.write_u16(0x62, 0xF000 | 2 << 6);
        apu.write_u16(0x64, 0x8000 | 1792);
        let samples = run_samples(&mut apu, 4096);
        let left: Vec<i16> = samples.iter().step_by(2).copied().collect();
        let high = *left.iter().max().unwrap();
        let low = *left.iter().min().unwrap();
        assert!(high > 0 && low <= 0, "high {} low {}", high, low);
        let transitions = left.windows(2).filter(|pair| pair[0] != pair[1]).count();
        let expected_period_samples = SAMPLE_RATE as f64 / 512.0;
        let expected_transitions = (left.len() as f64 / expected_period_samples * 2.0) as usize;
        assert!((transitions as i64 - expected_transitions as i64).abs() <= 2, "{} transitions, expected {}", transitions, expected_transitions);
    }

    #[test]
    fn test_master_disable_is_silent() {
        let mut apu = Apu::new();
        apu.write_u16(0x62, 0xF000 | 2 << 6);
        apu.write_u16(0x64, 0x8000 | 1792);
        assert!(run_samples(&mut apu, 256).iter().all(|sample| *sample == 0));
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
        let (left, right) = apu.mix();
        assert_eq!((left, right), ((0x60 * 2) << 6, (0x60 * 2) << 6));
        for _ in 0..14 {
            assert!(!apu.timer_overflow(0)[0]);
        }
        assert!(apu.timer_overflow(0)[0]);
        assert_eq!(apu.mix(), (0, 0));
    }

    #[test]
    fn test_noise_produces_varying_output() {
        let mut apu = enabled_apu();
        apu.write_u16(0x78, 0xF000);
        apu.write_u16(0x7C, 0x8000 | 0x10);
        let samples = run_samples(&mut apu, 2048);
        assert!(samples.iter().any(|sample| *sample != samples[0]));
    }
}
