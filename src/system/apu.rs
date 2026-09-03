use std::collections::VecDeque;

use super::{
    cpu::CPU_FREQUENCY,
    state::{Reader, StateError, Writer},
    synth::{kernel, Synth, FRACTION_BITS, KERNEL_SHIFT, LEVEL_BITS, TAPS},
};

pub const SAMPLE_RATE: u32 = 48_000;
const FRAME_SEQUENCER_CYCLES: u32 = 32_768;
const GRID_CYCLES: u32 = 64;
const FLUSH_CYCLES: u32 = CPU_FREQUENCY as u32 / 8;
const LEVEL_SCALE: i32 = 1 << (LEVEL_BITS - 10);
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

    fn save_state(&self, writer: &mut Writer) {
        writer.bool(self.enabled);
        writer.u16(self.counter);
    }

    fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.enabled = reader.bool()?;
        self.counter = reader.u16()?;
        Ok(())
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

    fn save_state(&self, writer: &mut Writer) {
        self.sweep.save_state(writer);
        writer.u8(self.duty);
        self.length.save_state(writer);
        self.envelope.save_state(writer);
        writer.u16(self.frequency);
        writer.bool(self.enabled);
        writer.u8(self.phase);
        writer.i32(self.cycles);
    }

    fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.sweep.load_state(reader)?;
        self.duty = reader.u8()? & 0b11;
        self.length.load_state(reader)?;
        self.envelope.load_state(reader)?;
        self.frequency = reader.u16()? & 0x7FF;
        self.enabled = reader.bool()?;
        self.phase = reader.u8()? & 7;
        self.cycles = reader.i32()?;
        Ok(())
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

    fn save_state(&self, writer: &mut Writer) {
        writer.bool(self.two_banks);
        writer.u8(self.playing_bank as u8);
        writer.bool(self.playback);
        self.length.save_state(writer);
        writer.u8(self.volume);
        writer.bool(self.force_75);
        writer.u16(self.frequency);
        writer.bool(self.enabled);
        writer.bytes(&self.ram[0]);
        writer.bytes(&self.ram[1]);
        writer.u8(self.position);
        writer.i32(self.cycles);
    }

    fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.two_banks = reader.bool()?;
        self.playing_bank = reader.u8()? as usize & 1;
        self.playback = reader.bool()?;
        self.length.load_state(reader)?;
        self.volume = reader.u8()?;
        self.force_75 = reader.bool()?;
        self.frequency = reader.u16()? & 0x7FF;
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

    fn sample(&self) -> u8 {
        if self.enabled && self.lfsr & 1 == 0 {
            self.envelope.volume
        } else {
            0
        }
    }
}

#[derive(Debug)]
struct Fifo {
    samples: VecDeque<i8>,
    current: i8,
    level: i32,
    history: [i8; TAPS],
    head: usize,
    last_pop: u64,
    interval: u64,
}

impl Fifo {
    fn new() -> Fifo {
        Fifo {
            samples: VecDeque::new(),
            current: 0,
            level: 0,
            history: [0; TAPS],
            head: 0,
            last_pop: 0,
            interval: 0,
        }
    }

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
        self.head = (self.head + 1) % TAPS;
        self.history[self.head] = self.current;
        self.interval = if self.last_pop == 0 { 0 } else { clock - self.last_pop };
        self.last_pop = clock;
        if !smooth {
            self.level = self.current as i32 * LEVEL_SCALE;
        }
        self.samples.len() <= FIFO_REFILL_THRESHOLD
    }

    fn interpolate(&self, clock: u64) -> i32 {
        if self.interval == 0 {
            return self.current as i32 * LEVEL_SCALE;
        }
        let elapsed = ((clock - self.last_pop) << FRACTION_BITS) / self.interval;
        let weights = kernel().weights((1 << FRACTION_BITS) - elapsed.min(1 << FRACTION_BITS) as u32);
        let mut total = 0i32;
        for (k, weight) in weights.iter().enumerate() {
            total += weight * self.history[(self.head + TAPS - k) % TAPS] as i32;
        }
        total >> (KERNEL_SHIFT - LEVEL_SCALE.trailing_zeros())
    }

    fn restart(&mut self) {
        self.level = self.current as i32 * LEVEL_SCALE;
        self.history = [0; TAPS];
        self.head = 0;
        self.last_pop = 0;
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
    clock: u64,
    frame_cycles: u32,
    grid_cycles: u32,
    smooth: bool,
    last_level: [i32; 2],
    synth: [Synth; 2],
}

impl Apu {
    pub fn new() -> Apu {
        Apu {
            square1: Square::new(),
            square2: Square::new(),
            wave: Wave::new(),
            noise: Noise::new(),
            fifo: [Fifo::new(), Fifo::new()],
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
            clock: 0,
            frame_cycles: 0,
            grid_cycles: 0,
            smooth: false,
            last_level: [0; 2],
            synth: [Synth::new(SAMPLE_RATE), Synth::new(SAMPLE_RATE)],
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
                refill[fifo] = self.fifo[fifo].pop(self.clock, self.smooth);
            }
        }
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
            self.clock += step as u64;
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
        self.square1.advance(GRID_CYCLES as i32);
        self.square2.advance(GRID_CYCLES as i32);
        self.wave.advance(GRID_CYCLES as i32);
        self.noise.advance(GRID_CYCLES as i32);
    }

    fn emit(&mut self) {
        let (left, right) = self.mix();
        if left != self.last_level[0] {
            self.synth[0].add_delta(self.frame_cycles, left - self.last_level[0]);
            self.last_level[0] = left;
        }
        if right != self.last_level[1] {
            self.synth[1].add_delta(self.frame_cycles, right - self.last_level[1]);
            self.last_level[1] = right;
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

    fn mix(&self) -> (i32, i32) {
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
            let psg_level = (psg_sum * volume as i32 * 4 * LEVEL_SCALE / 7) >> (2 - self.psg_scale.min(2));
            let mut level = (self.bias & 0x3FE) as i32 * LEVEL_SCALE + psg_level;
            for fifo in 0..2 {
                if fifo_right_or_left[fifo] {
                    let sample = self.fifo[fifo].level;
                    level += if self.fifo_full_volume[fifo] { sample * 2 } else { sample };
                }
            }
            level.clamp(0, 0x400 * LEVEL_SCALE - 1) - 0x200 * LEVEL_SCALE
        };
        (
            side(self.psg_volume_left, self.psg_enable_left, self.fifo_enable_left),
            side(self.psg_volume_right, self.psg_enable_right, self.fifo_enable_right),
        )
    }

    pub fn save_state(&self, writer: &mut Writer) {
        self.square1.save_state(writer);
        self.square2.save_state(writer);
        self.wave.save_state(writer);
        self.noise.save_state(writer);
        self.fifo[0].save_state(writer);
        self.fifo[1].save_state(writer);
        writer.u8(self.psg_volume_right);
        writer.u8(self.psg_volume_left);
        writer.u8(self.psg_enable_right);
        writer.u8(self.psg_enable_left);
        writer.u8(self.psg_scale);
        writer.bools(&self.fifo_full_volume);
        writer.bools(&self.fifo_enable_right);
        writer.bools(&self.fifo_enable_left);
        writer.bytes(&self.fifo_timer);
        writer.bool(self.master_enable);
        writer.u16(self.bias);
        writer.u32(self.frame_sequencer_cycles);
        writer.u8(self.frame_sequencer_step);
        writer.u64(self.synth[0].phase(self.frame_cycles));
    }

    pub fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.square1.load_state(reader)?;
        self.square2.load_state(reader)?;
        self.wave.load_state(reader)?;
        self.noise.load_state(reader)?;
        self.fifo[0].load_state(reader)?;
        self.fifo[1].load_state(reader)?;
        self.psg_volume_right = reader.u8()?;
        self.psg_volume_left = reader.u8()?;
        self.psg_enable_right = reader.u8()?;
        self.psg_enable_left = reader.u8()?;
        self.psg_scale = reader.u8()?;
        reader.bools(&mut self.fifo_full_volume)?;
        reader.bools(&mut self.fifo_enable_right)?;
        reader.bools(&mut self.fifo_enable_left)?;
        reader.bytes_into(&mut self.fifo_timer)?;
        self.master_enable = reader.bool()?;
        self.bias = reader.u16()?;
        self.frame_sequencer_cycles = reader.u32()?;
        self.frame_sequencer_step = reader.u8()? & 7;
        let phase = reader.u64()?;
        for synth in &mut self.synth {
            synth.set_phase(phase);
        }
        self.clock = 0;
        self.frame_cycles = 0;
        self.grid_cycles = 0;
        let (left, right) = self.mix();
        self.last_level = [left, right];
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
            fifo.level = fifo.current as i32 * LEVEL_SCALE;
        }
    }

    pub fn take_samples(&mut self) -> Vec<i16> {
        self.flush_synth();
        let (left, right) = (self.synth[0].take(), self.synth[1].take());
        left.iter().zip(&right).flat_map(|(left, right)| [*left, *right]).collect()
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
        let expected_period_samples = SAMPLE_RATE as f64 / 512.0;
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
            let left: Vec<i32> = apu.take_samples().iter().step_by(2).map(|sample| *sample as i32).collect();
            let edge = left.iter().position(|sample| *sample > left.iter().max().unwrap() / 2).unwrap();
            let level = left[edge + 20];
            let rising = left[edge - 20..edge + 20].iter().filter(|sample| **sample > level / 8 && **sample < level * 7 / 8).count();
            (level, rising)
        };
        let (exact_level, exact_rising) = rise(false);
        let (smooth_level, smooth_rising) = rise(true);
        assert!((exact_level - smooth_level).abs() < exact_level / 8, "levels {} and {}", exact_level, smooth_level);
        assert!(exact_rising <= 2, "{} samples rising in exact mode", exact_rising);
        assert!(smooth_rising >= 6, "{} samples rising in smooth mode", smooth_rising);
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
        assert_eq!(apu.mix(), (0, 0));
        apu.write_u16(0x82, 0x2 | 0x4 | 0x100 | 0x200);
        apu.write_fifo(0, 0x0000_0060);
        apu.timer_overflow(0);
        assert_eq!(apu.mix(), (0x60 * 2 * LEVEL_SCALE, 0x60 * 2 * LEVEL_SCALE));
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
        assert_eq!(apu.mix(), (0x60 * 2 * LEVEL_SCALE, 0x60 * 2 * LEVEL_SCALE));
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
