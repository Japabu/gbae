use crate::bits::Bits;

use super::{
    cpu::CPU,
    memory::{DmaTiming, Interrupt, Key, Memory},
    ppu::{Framebuffer, PPU},
    save::SaveType,
    state::{Reader, StateError, Writer},
};

pub const CYCLES_PER_SCANLINE: u64 = 1232;
pub const HDRAW_CYCLES: u64 = 960;
pub const SCANLINES_PER_FRAME: u64 = 228;
pub const VISIBLE_SCANLINES: u16 = 160;

const DISPSTAT_VBLANK: u32 = 0;
const DISPSTAT_HBLANK: u32 = 1;
const DISPSTAT_VCOUNT_MATCH: u32 = 2;
const DISPSTAT_VBLANK_IRQ: u32 = 3;
const DISPSTAT_HBLANK_IRQ: u32 = 4;
const DISPSTAT_VCOUNT_IRQ: u32 = 5;
const DISPSTAT_VCOUNT_SETTING: std::ops::Range<u32> = 8..16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Draw,
    Blank,
}

pub struct Gba {
    pub cpu: CPU,
    pub mem: Memory,
    pub ppu: PPU,
    scanline_counter: u64,
    next_event_cycles: u64,
    phase: Phase,
}

impl Gba {
    pub fn new(cartridge_data: Vec<u8>) -> Gba {
        let mut gba = Gba {
            cpu: CPU::new(),
            mem: Memory::new(cartridge_data),
            ppu: PPU::new(),
            scanline_counter: 0,
            next_event_cycles: HDRAW_CYCLES,
            phase: Phase::Draw,
        };
        gba.cpu.reset(&mut gba.mem);
        gba.start_scanline();
        gba
    }

    pub fn step(&mut self) {
        while self.cpu.cycles() >= self.next_event_cycles {
            self.advance_event();
        }
        let stalled = self.mem.take_cycles();
        self.cpu.add_cycles(u64::from(stalled));
        self.mem.tick(stalled);
        let io = self.mem.io_mut();
        if io.halted && io.ie & io.irf == 0 {
            let skipped = self.next_event_cycles.saturating_sub(self.cpu.cycles());
            self.cpu.add_cycles(skipped);
            self.mem.tick(skipped as u32);
        } else {
            io.halted = false;
            self.cpu.handle_interrupts(&mut self.mem);
            let before = self.cpu.cycles();
            self.cpu.cycle(&mut self.mem);
            self.mem.tick((self.cpu.cycles() - before) as u32);
        }
    }

    fn advance_event(&mut self) {
        match self.phase {
            Phase::Draw => {
                self.phase = Phase::Blank;
                self.next_event_cycles += CYCLES_PER_SCANLINE - HDRAW_CYCLES;
                self.start_hblank();
            }
            Phase::Blank => {
                self.phase = Phase::Draw;
                self.scanline_counter += 1;
                self.next_event_cycles += HDRAW_CYCLES;
                self.start_scanline();
            }
        }
    }

    fn start_scanline(&mut self) {
        let v_count = (self.scanline_counter % SCANLINES_PER_FRAME) as u16;
        let io = self.mem.io_mut();
        io.v_count = v_count;
        let in_vblank = (VISIBLE_SCANLINES..SCANLINES_PER_FRAME as u16 - 1).contains(&v_count);
        let vcount_match = v_count == io.disp_stat.bits(DISPSTAT_VCOUNT_SETTING);
        io.disp_stat = io
            .disp_stat
            .with_bit(DISPSTAT_VBLANK, in_vblank)
            .with_bit(DISPSTAT_HBLANK, false)
            .with_bit(DISPSTAT_VCOUNT_MATCH, vcount_match);
        if vcount_match && io.disp_stat.bit(DISPSTAT_VCOUNT_IRQ) {
            io.raise(Interrupt::VCount);
        }
        if v_count == VISIBLE_SCANLINES && io.disp_stat.bit(DISPSTAT_VBLANK_IRQ) {
            io.raise(Interrupt::VBlank);
        }
        self.ppu.latch_affine_references(io);
        if v_count == VISIBLE_SCANLINES {
            self.ppu.finish_frame();
            self.mem.start_dma(DmaTiming::VBlank);
        }
    }

    fn start_hblank(&mut self) {
        let io = self.mem.io_mut();
        io.disp_stat = io.disp_stat.with_bit(DISPSTAT_HBLANK, true);
        if io.disp_stat.bit(DISPSTAT_HBLANK_IRQ) {
            io.raise(Interrupt::HBlank);
        }
        if io.v_count < VISIBLE_SCANLINES {
            self.ppu.draw_scanline(&self.mem);
            self.mem.start_dma(DmaTiming::HBlank);
        }
    }

    pub fn run_scanline(&mut self) {
        self.run_to_scanline(self.scanline_counter + 1);
    }

    pub fn run_frame(&mut self) {
        self.run_to_scanline((self.frame_count() + 1) * SCANLINES_PER_FRAME);
    }

    fn run_to_scanline(&mut self, target: u64) {
        while self.scanline_counter < target {
            self.step();
        }
    }

    pub fn run_until(&mut self, mut condition: impl FnMut(&Gba) -> bool, max_steps: u64) -> bool {
        for _ in 0..max_steps {
            if condition(self) {
                return true;
            }
            self.step();
        }
        condition(self)
    }

    pub fn set_key(&mut self, key: Key, pressed: bool) {
        let keys = self.mem.io().pressed_keys().with_bits(key.number()..key.number() + 1, u16::from(pressed));
        self.mem.io_mut().set_pressed_keys(keys);
    }

    pub fn set_pressed_keys(&mut self, keys: u16) {
        self.mem.io_mut().set_pressed_keys(keys);
    }

    pub fn save_type(&self) -> SaveType {
        self.mem.save_type()
    }

    pub fn save_data(&self) -> &[u8] {
        self.mem.save_data()
    }

    pub fn load_save_data(&mut self, bytes: &[u8]) {
        self.mem.load_save_data(bytes);
    }

    pub fn take_save_dirty(&mut self) -> bool {
        self.mem.take_save_dirty()
    }

    pub fn set_time(&mut self, unix_seconds: u64) {
        self.mem.set_time(unix_seconds);
    }

    pub fn save_state(&self) -> Vec<u8> {
        let mut writer = Writer::new();
        writer.sized_bytes(&self.mem.rom_identity());
        self.cpu.save_state(&mut writer);
        self.mem.save_state(&mut writer);
        self.ppu.save_state(&mut writer);
        writer.u64(self.scanline_counter);
        writer.u64(self.next_event_cycles);
        writer.bool(self.phase == Phase::Blank);
        writer.finish()
    }

    pub fn load_state(&mut self, bytes: &[u8]) -> Result<(), StateError> {
        let mut reader = Reader::new(bytes)?;
        if reader.sized_bytes()? != self.mem.rom_identity() {
            return Err(StateError::DifferentRom);
        }
        self.cpu.load_state(&mut reader)?;
        self.mem.load_state(&mut reader)?;
        self.ppu.load_state(&mut reader)?;
        self.scanline_counter = reader.u64()?;
        self.next_event_cycles = reader.u64()?;
        self.phase = if reader.bool()? { Phase::Blank } else { Phase::Draw };
        Ok(())
    }

    pub fn set_audio_sample_rate(&mut self, sample_rate: u32) {
        self.mem.flush_apu();
        self.mem.apu.set_sample_rate(sample_rate);
    }

    pub fn take_audio_samples(&mut self) -> Vec<i16> {
        self.mem.flush_apu();
        self.mem.apu.take_samples()
    }

    pub fn framebuffer(&self) -> &Framebuffer {
        self.ppu.framebuffer()
    }

    pub fn scanline(&self) -> u64 {
        self.scanline_counter
    }

    pub fn frame_count(&self) -> u64 {
        self.scanline_counter / SCANLINES_PER_FRAME
    }
}
