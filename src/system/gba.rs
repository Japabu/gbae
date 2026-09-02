use super::{
    cpu::CPU,
    memory::{DmaTiming, Memory},
    ppu::{Framebuffer, PPU},
};

pub const CYCLES_PER_SCANLINE: u64 = 1232;
pub const HDRAW_CYCLES: u64 = 960;
pub const SCANLINES_PER_FRAME: u64 = 228;
pub const VISIBLE_SCANLINES: u16 = 160;

const DISPSTAT_VBLANK: u16 = 1 << 0;
const DISPSTAT_HBLANK: u16 = 1 << 1;
const DISPSTAT_VCOUNT: u16 = 1 << 2;
const DISPSTAT_VBLANK_IRQ: u16 = 1 << 3;
const DISPSTAT_HBLANK_IRQ: u16 = 1 << 4;
const DISPSTAT_VCOUNT_IRQ: u16 = 1 << 5;

const IRQ_VBLANK: u16 = 1 << 0;
const IRQ_HBLANK: u16 = 1 << 1;
const IRQ_VCOUNT: u16 = 1 << 2;

pub struct Gba {
    pub cpu: CPU,
    pub mem: Memory,
    pub ppu: PPU,
    scanline_counter: u64,
    next_event_cycles: u64,
    in_hblank: bool,
}

impl Gba {
    pub fn new(bios: Vec<u8>, cartridge_data: Vec<u8>) -> Self {
        let mut gba = Self {
            cpu: CPU::new(),
            mem: Memory::new(bios, cartridge_data),
            ppu: PPU::new(),
            scanline_counter: 0,
            next_event_cycles: HDRAW_CYCLES,
            in_hblank: false,
        };
        gba.cpu.reset(&mut gba.mem);
        gba.start_scanline();
        gba
    }

    #[inline]
    pub fn step(&mut self) {
        while self.cpu.get_cycles() >= self.next_event_cycles {
            self.advance_event();
        }
        let io = self.mem.get_io_registers_mut();
        if io.halted && io.ie & io.irf == 0 {
            let skipped = self.next_event_cycles - self.cpu.get_cycles();
            io.tick_timers(skipped as u32);
            self.cpu.add_cycles(skipped);
        } else {
            io.halted = false;
            self.cpu.handle_interrupts(&mut self.mem);
            let cycles_before = self.cpu.get_cycles();
            self.cpu.cycle(&mut self.mem);
            let elapsed = (self.cpu.get_cycles() - cycles_before) as u32;
            self.mem.get_io_registers_mut().tick_timers(elapsed);
        }
    }

    fn advance_event(&mut self) {
        if self.in_hblank {
            self.in_hblank = false;
            self.scanline_counter += 1;
            self.next_event_cycles += HDRAW_CYCLES;
            self.start_scanline();
        } else {
            self.in_hblank = true;
            self.next_event_cycles += CYCLES_PER_SCANLINE - HDRAW_CYCLES;
            self.start_hblank();
        }
    }

    fn start_scanline(&mut self) {
        let v_count = (self.scanline_counter % SCANLINES_PER_FRAME) as u16;
        let io = self.mem.get_io_registers_mut();
        io.v_count = v_count;
        io.disp_stat &= !(DISPSTAT_VBLANK | DISPSTAT_HBLANK | DISPSTAT_VCOUNT);

        if v_count >= VISIBLE_SCANLINES && v_count < SCANLINES_PER_FRAME as u16 - 1 {
            io.disp_stat |= DISPSTAT_VBLANK;
        }
        if v_count == io.disp_stat >> 8 {
            io.disp_stat |= DISPSTAT_VCOUNT;
            if io.disp_stat & DISPSTAT_VCOUNT_IRQ != 0 {
                io.irf |= IRQ_VCOUNT;
            }
        }
        if v_count == VISIBLE_SCANLINES && io.disp_stat & DISPSTAT_VBLANK_IRQ != 0 {
            io.irf |= IRQ_VBLANK;
        }
        self.ppu.latch_affine_references(io);

        if v_count == VISIBLE_SCANLINES {
            self.mem.start_dma(DmaTiming::VBlank);
        }
    }

    fn start_hblank(&mut self) {
        let io = self.mem.get_io_registers_mut();
        io.disp_stat |= DISPSTAT_HBLANK;
        if io.disp_stat & DISPSTAT_HBLANK_IRQ != 0 {
            io.irf |= IRQ_HBLANK;
        }
        if io.v_count < VISIBLE_SCANLINES {
            self.ppu.draw_scanline(&self.mem);
            self.mem.start_dma(DmaTiming::HBlank);
        }
    }

    pub fn run_scanline(&mut self) {
        let target = self.scanline_counter + 1;
        while self.scanline_counter < target {
            self.step();
        }
    }

    pub fn run_frame(&mut self) {
        let target = self.scanline_counter + SCANLINES_PER_FRAME;
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
