/*
GBA Memory Map
General Internal Memory
  00_000_000-00_003_FFF   BIOS - System ROM         (16 KBytes)
  00_004_000-01_FFF_FFF   Not used
  02_000_000-02_03F_FFF   WRAM - On-board Work RAM  (256 KBytes) 2 Wait
  02_040_000-02_FFF_FFF   Not used
  03_000_000-03_007_FFF   WRAM - On-chip Work RAM   (32 KBytes)
  03_008_000-03_FFF_FFF   Not used
  04_000_000-04_000_3FE   I/O Registers
  04_000_400-04_FFF_FFF   Not used
Internal Display Memory
  05_000_000-05_000_3FF   BG/OBJ Palette RAM        (1 Kbyte)
  05_000_400-05_FFF_FFF   Not used
  06_000_000-06_017_FFF   VRAM - Video RAM          (96 KBytes)
  06_018_000-06_FFF_FFF   Not used
  07_000_000-07_000_3FF   OAM - OBJ Attributes      (1 Kbyte)
  07_000_400-07_FFF_FFF   Not used
External Memory (Game Pak)
  08_000_000-09_FFF_FFF   Game Pak ROM/FlashROM (max 32MB) - Wait State 0
  0A_000_000-0B_FFF_FFF   Game Pak ROM/FlashROM (max 32MB) - Wait State 1
  0C_000_000-0D_FFF_FFF   Game Pak ROM/FlashROM (max 32MB) - Wait State 2
  0E_000_000-0E_00F_FFF   Game Pak SRAM    (max 64 KBytes) - 8bit Bus width
  0E_010_000-0F_FFF_FFF   Not used
Unused Memory Area
  10_000_000-FF_FFF_FFF   Not used (upper 4bits of address bus unused)
*/

use std::fmt::Display;

pub const BIOS_LEN: usize = 0x4000;
pub const WRAM1_LEN: usize = 0x4_0000;
pub const WRAM2_LEN: usize = 0x8000;
pub const PALETTE_RAM_LEN: usize = 0x400;
pub const VRAM_LEN: usize = 0x1_8000;
pub const OAM_LEN: usize = 0x400;
pub const FLASH_LEN: usize = 0x2_0000;

#[inline(always)]
fn read_halfword(buffer: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([buffer[offset], buffer[offset + 1]])
}

#[inline(always)]
fn read_word(buffer: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([buffer[offset], buffer[offset + 1], buffer[offset + 2], buffer[offset + 3]])
}

#[inline(always)]
fn write_halfword(buffer: &mut [u8], offset: usize, value: u16) {
    buffer[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

#[inline(always)]
fn write_word(buffer: &mut [u8], offset: usize, value: u32) {
    buffer[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[inline(always)]
fn vram_offset(address: u32) -> usize {
    let offset = (address & 0x1_FFFF) as usize;
    if offset >= VRAM_LEN {
        offset - 0x8000
    } else {
        offset
    }
}

fn boxed_zeroed<const N: usize>() -> Box<[u8; N]> {
    vec![0u8; N].into_boxed_slice().try_into().unwrap()
}

#[derive(PartialEq)]
enum FlashCommand {
    None,
    Prefix1,
    Prefix2,
    ErasePrefix1,
    ErasePrefix2,
    ErasePrefix3,
    Program,
    BankSwitch,
}

pub struct FlashRegion {
    data: Vec<u8>,
    bank: usize,
    id_mode: bool,
    command: FlashCommand,
}

impl FlashRegion {
    pub fn new(size: usize) -> Self {
        Self {
            data: vec![0xFF; size],
            bank: 0,
            id_mode: false,
            command: FlashCommand::None,
        }
    }

    fn address(&self, offset: u32) -> usize {
        self.bank * 0x10000 + (offset & 0xFFFF) as usize
    }

    fn read_u8(&self, offset: u32) -> u8 {
        if self.id_mode {
            match offset & 0xFFFF {
                0 => 0xC2,
                1 => 0x09,
                _ => 0,
            }
        } else {
            self.data[self.address(offset)]
        }
    }

    fn write_u8(&mut self, offset: u32, value: u8) {
        let offset = offset & 0xFFFF;
        match self.command {
            FlashCommand::None if offset == 0x5555 && value == 0xAA => self.command = FlashCommand::Prefix1,
            FlashCommand::Prefix1 if offset == 0x2AAA && value == 0x55 => self.command = FlashCommand::Prefix2,
            FlashCommand::Prefix2 if offset == 0x5555 => {
                self.command = match value {
                    0x80 => FlashCommand::ErasePrefix1,
                    0xA0 => FlashCommand::Program,
                    0xB0 => FlashCommand::BankSwitch,
                    _ => FlashCommand::None,
                };
                match value {
                    0x90 => self.id_mode = true,
                    0xF0 => self.id_mode = false,
                    _ => {}
                }
            }
            FlashCommand::ErasePrefix1 if offset == 0x5555 && value == 0xAA => self.command = FlashCommand::ErasePrefix2,
            FlashCommand::ErasePrefix2 if offset == 0x2AAA && value == 0x55 => self.command = FlashCommand::ErasePrefix3,
            FlashCommand::ErasePrefix3 => {
                if offset == 0x5555 && value == 0x10 {
                    self.data.fill(0xFF);
                } else if value == 0x30 {
                    let start = self.bank * 0x10000 + (offset & 0xF000) as usize;
                    self.data[start..start + 0x1000].fill(0xFF);
                }
                self.command = FlashCommand::None;
            }
            FlashCommand::Program => {
                let address = self.address(offset);
                self.data[address] &= value;
                self.command = FlashCommand::None;
            }
            FlashCommand::BankSwitch => {
                if offset == 0 {
                    self.bank = value as usize % (self.data.len() / 0x10000);
                }
                self.command = FlashCommand::None;
            }
            _ => self.command = FlashCommand::None,
        }
    }
}

pub struct IoRegisters {
    pub disp_cnt: u16,
    pub disp_stat: u16,
    pub v_count: u16,
    pub bg_cnt: [u16; 4],
    pub bg_h_offset: [u16; 4],
    pub bg_v_offset: [u16; 4],
    pub bg_parameters: [[u16; 4]; 2],
    pub bg_reference: [[u32; 2]; 2],
    pub bg_reference_written: [bool; 2],
    pub win_h: [u16; 2],
    pub win_v: [u16; 2],
    pub win_in: u16,
    pub win_out: u16,
    pub mosaic: u16,
    pub blend_cnt: u16,
    pub blend_alpha: u16,
    pub blend_y: u16,
    sound3_cnt_l: u16,
    sound_cnt_l: u16,
    sound_cnt_h: u16,
    sound_cnt_x: u16,
    soundbias: u16,
    wave_ram: [u8; 16],
    fifo_a: u32,
    fifo_b: u32,
    pub dma_sad: [u32; 4],
    pub dma_dad: [u32; 4],
    pub dma_cnt_l: [u16; 4],
    pub dma_cnt_h: [u16; 4],
    tm_reload: [u16; 4],
    tm_control: [u16; 4],
    tm_counter: [u16; 4],
    tm_cycles: [u32; 4],
    sio_data32: u32,
    sio_multi2: u16,
    sio_multi3: u16,
    sio_cnt: u16,
    sio_data8: u16,
    pub key_input: u16,
    key_cnt: u16,
    rcnt: u16,
    joy_cnt: u16,
    joy_recv: u32,
    joy_trans: u32,
    joy_stat: u16,
    pub ie: u16,
    pub irf: u16,
    wait_cnt: u16,
    pub ime: bool,
    post_flg: bool,
    pub halted: bool,
}

impl IoRegisters {
    pub fn new() -> Self {
        Self {
            disp_cnt: 0,
            disp_stat: 0,
            v_count: 0,
            bg_cnt: [0; 4],
            bg_h_offset: [0; 4],
            bg_v_offset: [0; 4],
            bg_parameters: [[0x100, 0, 0, 0x100]; 2],
            bg_reference: [[0; 2]; 2],
            bg_reference_written: [false; 2],
            win_h: [0; 2],
            win_v: [0; 2],
            win_in: 0,
            win_out: 0,
            mosaic: 0,
            blend_cnt: 0,
            blend_alpha: 0,
            blend_y: 0,
            sound3_cnt_l: 0,
            sound_cnt_l: 0,
            sound_cnt_h: 0,
            sound_cnt_x: 0,
            soundbias: 0x0200,
            wave_ram: [0; 16],
            fifo_a: 0,
            fifo_b: 0,
            dma_sad: [0; 4],
            dma_dad: [0; 4],
            dma_cnt_l: [0; 4],
            dma_cnt_h: [0; 4],
            tm_reload: [0; 4],
            tm_control: [0; 4],
            tm_counter: [0; 4],
            tm_cycles: [0; 4],
            sio_data32: 0,
            sio_multi2: 0,
            sio_multi3: 0,
            sio_cnt: 0,
            sio_data8: 0,
            key_input: 0x03FF,
            key_cnt: 0,
            rcnt: 0,
            joy_cnt: 0,
            joy_recv: 0,
            joy_trans: 0,
            joy_stat: 0,
            ie: 0,
            irf: 0,
            wait_cnt: 0,
            ime: false,
            post_flg: false,
            halted: false,
        }
    }

    pub fn read_u8(&self, offset: u32) -> u8 {
        (self.read_u16(offset & !1) >> (8 * (offset & 1))) as u8
    }

    pub fn read_u16(&self, offset: u32) -> u16 {
        match offset {
            0x000 => self.disp_cnt,
            0x004 => self.disp_stat,
            0x006 => self.v_count,
            0x008 => self.bg_cnt[0],
            0x00A => self.bg_cnt[1],
            0x00C => self.bg_cnt[2],
            0x00E => self.bg_cnt[3],
            0x048 => self.win_in,
            0x04A => self.win_out,
            0x050 => self.blend_cnt,
            0x052 => self.blend_alpha,
            0x070 => self.sound3_cnt_l,
            0x080 => self.sound_cnt_l,
            0x082 => self.sound_cnt_h,
            0x084 => self.sound_cnt_x,
            0x088 => self.soundbias,
            0x090..=0x09F => read_halfword(&self.wave_ram, (offset & 0xF) as usize),
            0x0B8 => self.dma_cnt_l[0],
            0x0BA => self.dma_cnt_h[0],
            0x0C4 => self.dma_cnt_l[1],
            0x0C6 => self.dma_cnt_h[1],
            0x0D0 => self.dma_cnt_l[2],
            0x0D2 => self.dma_cnt_h[2],
            0x0DC => self.dma_cnt_l[3],
            0x0DE => self.dma_cnt_h[3],
            0x100 => self.tm_counter[0],
            0x102 => self.tm_control[0],
            0x104 => self.tm_counter[1],
            0x106 => self.tm_control[1],
            0x108 => self.tm_counter[2],
            0x10A => self.tm_control[2],
            0x10C => self.tm_counter[3],
            0x10E => self.tm_control[3],
            0x120 => self.sio_data32 as u16,
            0x122 => (self.sio_data32 >> 16) as u16,
            0x124 => self.sio_multi2,
            0x126 => self.sio_multi3,
            0x128 => self.sio_cnt,
            0x12A => self.sio_data8,
            0x130 => self.key_input,
            0x132 => self.key_cnt,
            0x134 => self.rcnt,
            0x140 => self.joy_cnt,
            0x150 => self.joy_recv as u16,
            0x152 => (self.joy_recv >> 16) as u16,
            0x154 => self.joy_trans as u16,
            0x156 => (self.joy_trans >> 16) as u16,
            0x158 => self.joy_stat,
            0x200 => self.ie,
            0x202 => self.irf,
            0x204 => self.wait_cnt,
            0x208 => self.ime as u16,
            0x300 => self.post_flg as u16,
            _ => 0,
        }
    }

    pub fn read_u32(&self, offset: u32) -> u32 {
        match offset {
            0x0A0 => self.fifo_a,
            0x0A4 => self.fifo_b,
            0x0B0 => self.dma_sad[0],
            0x0B4 => self.dma_dad[0],
            0x0BC => self.dma_sad[1],
            0x0C0 => self.dma_dad[1],
            0x0C8 => self.dma_sad[2],
            0x0CC => self.dma_dad[2],
            0x0D4 => self.dma_sad[3],
            0x0D8 => self.dma_dad[3],
            _ => self.read_u16(offset) as u32 | (self.read_u16(offset + 2) as u32) << 16,
        }
    }

    pub fn write_u8(&mut self, offset: u32, value: u8) {
        match offset {
            0x202 | 0x203 => self.irf &= !((value as u16) << (8 * (offset & 1))),
            0x300 => self.post_flg = value & 1 != 0,
            0x301 => self.halted = value & 0x80 == 0,
            _ => {
                let old = self.read_u16(offset & !1);
                let new = if offset & 1 == 0 { old & 0xFF00 | value as u16 } else { old & 0x00FF | (value as u16) << 8 };
                self.write_u16(offset & !1, new)
            }
        }
    }

    pub fn write_u16(&mut self, offset: u32, value: u16) {
        match offset {
            0x000 => self.disp_cnt = value,
            0x004 => self.disp_stat = (self.disp_stat & 0x0007) | (value & 0xFFF8),
            0x008 => self.bg_cnt[0] = value,
            0x00A => self.bg_cnt[1] = value,
            0x00C => self.bg_cnt[2] = value,
            0x00E => self.bg_cnt[3] = value,
            0x010 => self.bg_h_offset[0] = value & 0x1FF,
            0x012 => self.bg_v_offset[0] = value & 0x1FF,
            0x014 => self.bg_h_offset[1] = value & 0x1FF,
            0x016 => self.bg_v_offset[1] = value & 0x1FF,
            0x018 => self.bg_h_offset[2] = value & 0x1FF,
            0x01A => self.bg_v_offset[2] = value & 0x1FF,
            0x01C => self.bg_h_offset[3] = value & 0x1FF,
            0x01E => self.bg_v_offset[3] = value & 0x1FF,
            0x020..=0x026 => self.bg_parameters[0][((offset - 0x020) / 2) as usize] = value,
            0x028 => self.write_bg_reference(0, 0, value, false),
            0x02A => self.write_bg_reference(0, 0, value, true),
            0x02C => self.write_bg_reference(0, 1, value, false),
            0x02E => self.write_bg_reference(0, 1, value, true),
            0x030..=0x036 => self.bg_parameters[1][((offset - 0x030) / 2) as usize] = value,
            0x038 => self.write_bg_reference(1, 0, value, false),
            0x03A => self.write_bg_reference(1, 0, value, true),
            0x03C => self.write_bg_reference(1, 1, value, false),
            0x03E => self.write_bg_reference(1, 1, value, true),
            0x040 => self.win_h[0] = value,
            0x042 => self.win_h[1] = value,
            0x044 => self.win_v[0] = value,
            0x046 => self.win_v[1] = value,
            0x048 => self.win_in = value,
            0x04A => self.win_out = value,
            0x04C => self.mosaic = value,
            0x050 => self.blend_cnt = value,
            0x052 => self.blend_alpha = value,
            0x054 => self.blend_y = value,
            0x070 => self.sound3_cnt_l = value,
            0x080 => self.sound_cnt_l = value,
            0x082 => self.sound_cnt_h = value,
            0x084 => self.sound_cnt_x = value,
            0x088 => self.soundbias = value,
            0x090..=0x09F => write_halfword(&mut self.wave_ram, (offset & 0xF) as usize, value),
            0x0A0 => self.fifo_a = self.fifo_a & 0xFFFF_0000 | value as u32,
            0x0A2 => self.fifo_a = self.fifo_a & 0x0000_FFFF | (value as u32) << 16,
            0x0A4 => self.fifo_b = self.fifo_b & 0xFFFF_0000 | value as u32,
            0x0A6 => self.fifo_b = self.fifo_b & 0x0000_FFFF | (value as u32) << 16,
            0x0B0 => self.dma_sad[0] = self.dma_sad[0] & 0xFFFF_0000 | value as u32,
            0x0B2 => self.dma_sad[0] = self.dma_sad[0] & 0x0000_FFFF | (value as u32) << 16,
            0x0B4 => self.dma_dad[0] = self.dma_dad[0] & 0xFFFF_0000 | value as u32,
            0x0B6 => self.dma_dad[0] = self.dma_dad[0] & 0x0000_FFFF | (value as u32) << 16,
            0x0B8 => self.dma_cnt_l[0] = value,
            0x0BA => self.dma_cnt_h[0] = value,
            0x0BC => self.dma_sad[1] = self.dma_sad[1] & 0xFFFF_0000 | value as u32,
            0x0BE => self.dma_sad[1] = self.dma_sad[1] & 0x0000_FFFF | (value as u32) << 16,
            0x0C0 => self.dma_dad[1] = self.dma_dad[1] & 0xFFFF_0000 | value as u32,
            0x0C2 => self.dma_dad[1] = self.dma_dad[1] & 0x0000_FFFF | (value as u32) << 16,
            0x0C4 => self.dma_cnt_l[1] = value,
            0x0C6 => self.dma_cnt_h[1] = value,
            0x0C8 => self.dma_sad[2] = self.dma_sad[2] & 0xFFFF_0000 | value as u32,
            0x0CA => self.dma_sad[2] = self.dma_sad[2] & 0x0000_FFFF | (value as u32) << 16,
            0x0CC => self.dma_dad[2] = self.dma_dad[2] & 0xFFFF_0000 | value as u32,
            0x0CE => self.dma_dad[2] = self.dma_dad[2] & 0x0000_FFFF | (value as u32) << 16,
            0x0D0 => self.dma_cnt_l[2] = value,
            0x0D2 => self.dma_cnt_h[2] = value,
            0x0D4 => self.dma_sad[3] = self.dma_sad[3] & 0xFFFF_0000 | value as u32,
            0x0D6 => self.dma_sad[3] = self.dma_sad[3] & 0x0000_FFFF | (value as u32) << 16,
            0x0D8 => self.dma_dad[3] = self.dma_dad[3] & 0xFFFF_0000 | value as u32,
            0x0DA => self.dma_dad[3] = self.dma_dad[3] & 0x0000_FFFF | (value as u32) << 16,
            0x0DC => self.dma_cnt_l[3] = value,
            0x0DE => self.dma_cnt_h[3] = value,
            0x100 => self.tm_reload[0] = value,
            0x102 => self.write_timer_control(0, value),
            0x104 => self.tm_reload[1] = value,
            0x106 => self.write_timer_control(1, value),
            0x108 => self.tm_reload[2] = value,
            0x10A => self.write_timer_control(2, value),
            0x10C => self.tm_reload[3] = value,
            0x10E => self.write_timer_control(3, value),
            0x120 => self.sio_data32 = self.sio_data32 & 0xFFFF_0000 | value as u32,
            0x122 => self.sio_data32 = self.sio_data32 & 0x0000_FFFF | (value as u32) << 16,
            0x124 => self.sio_multi2 = value,
            0x126 => self.sio_multi3 = value,
            0x128 => self.sio_cnt = value,
            0x12A => self.sio_data8 = value,
            0x132 => self.key_cnt = value,
            0x134 => self.rcnt = value,
            0x140 => self.joy_cnt = value,
            0x150 => self.joy_recv = self.joy_recv & 0xFFFF_0000 | value as u32,
            0x152 => self.joy_recv = self.joy_recv & 0x0000_FFFF | (value as u32) << 16,
            0x154 => self.joy_trans = self.joy_trans & 0xFFFF_0000 | value as u32,
            0x156 => self.joy_trans = self.joy_trans & 0x0000_FFFF | (value as u32) << 16,
            0x158 => self.joy_stat = value,
            0x200 => self.ie = value,
            0x202 => self.irf &= !value,
            0x204 => self.wait_cnt = value,
            0x208 => self.ime = value & 1 != 0,
            0x300 => {
                self.post_flg = value & 1 != 0;
                self.halted = value & 0x8000 == 0;
            }
            _ => {}
        }
    }

    pub fn write_u32(&mut self, offset: u32, value: u32) {
        self.write_u16(offset, value as u16);
        self.write_u16(offset + 2, (value >> 16) as u16);
    }

    fn write_bg_reference(&mut self, bg: usize, axis: usize, value: u16, high: bool) {
        let reference = &mut self.bg_reference[bg][axis];
        *reference = if high { *reference & 0x0000_FFFF | (value as u32) << 16 } else { *reference & 0xFFFF_0000 | value as u32 };
        self.bg_reference_written[bg] = true;
    }

    fn write_timer_control(&mut self, channel: usize, value: u16) {
        if value & 0x80 != 0 && self.tm_control[channel] & 0x80 == 0 {
            self.tm_counter[channel] = self.tm_reload[channel];
            self.tm_cycles[channel] = 0;
        }
        self.tm_control[channel] = value;
    }

    #[inline(always)]
    pub fn tick_timers(&mut self, cycles: u32) {
        if (self.tm_control[0] | self.tm_control[1] | self.tm_control[2] | self.tm_control[3]) & 0x80 != 0 {
            self.tick_enabled_timers(cycles);
        }
    }

    fn tick_enabled_timers(&mut self, cycles: u32) {
        for channel in 0..4 {
            let control = self.tm_control[channel];
            if control & 0x80 == 0 || (channel != 0 && control & 0x4 != 0) {
                continue;
            }
            let shift = match control & 3 {
                0 => 0,
                1 => 6,
                2 => 8,
                _ => 10,
            };
            let accumulated = self.tm_cycles[channel] + cycles;
            self.tm_cycles[channel] = accumulated & ((1 << shift) - 1);
            self.increment_timer(channel, accumulated >> shift);
        }
    }

    fn increment_timer(&mut self, channel: usize, ticks: u32) {
        let mut ticks = ticks;
        while ticks > 0 {
            let counter = self.tm_counter[channel] as u32;
            let space = 0x10000 - counter;
            if ticks < space {
                self.tm_counter[channel] = (counter + ticks) as u16;
                return;
            }
            ticks -= space;
            self.tm_counter[channel] = self.tm_reload[channel];
            self.timer_overflowed(channel);
        }
    }

    fn timer_overflowed(&mut self, channel: usize) {
        if self.tm_control[channel] & 0x40 != 0 {
            self.irf |= 1 << (3 + channel);
        }
        if channel < 3 && self.tm_control[channel + 1] & 0x84 == 0x84 {
            self.increment_timer(channel + 1, 1);
        }
    }
}

impl Display for IoRegisters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cnt = self.disp_cnt;
        let stat = self.disp_stat;

        writeln!(
            f,
            "DISPCNT  = {:#06X}\n\
             MODE CGB FRM HBLK VRAM FBLK BG0 BG1 BG2 BG3 OBJ W0 W1 OW\n\
             {:>4} {:>3} {:>3}  {:>4}  {:>4} {:>4}  {:>3} {:>3} {:>3} {:>3} {:>3} {:>2} {:>2} {:>2}",
            cnt,
            cnt & 0b111,
            (cnt >> 3) & 1,
            (cnt >> 4) & 1,
            (cnt >> 5) & 1,
            if ((cnt >> 6) & 1) == 1 { "1D" } else { "2D" },
            (cnt >> 7) & 1,
            (cnt >> 8) & 1,
            (cnt >> 9) & 1,
            (cnt >> 10) & 1,
            (cnt >> 11) & 1,
            (cnt >> 12) & 1,
            (cnt >> 13) & 1,
            (cnt >> 14) & 1,
            (cnt >> 15) & 1
        )?;

        writeln!(
            f,
            "DISPSTAT = {:#06X}\n\
             VB HB VC VBI HBI VCI MSB LYC\n\
             {:>2} {:>2} {:>2}  {:>2}  {:>2}  {:>2}  {:>3} {:>3}",
            stat,
            (stat >> 0) & 1,
            (stat >> 1) & 1,
            (stat >> 2) & 1,
            (stat >> 3) & 1,
            (stat >> 4) & 1,
            (stat >> 5) & 1,
            (stat >> 7) & 1,
            (stat >> 8) & 0xFF
        )?;

        writeln!(f, "V_COUNT = {v_count}", v_count = self.v_count)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegionKey {
    Bios(usize),
    Wram1(usize),
    Wram2(usize),
    IoRegisters(u32),
    PaletteRam(usize),
    Vram(usize),
    Oam(usize),
    GamePak(usize),
    Flash(u32),
    Unmapped,
}

pub struct Memory {
    bios: Box<[u8; BIOS_LEN]>,
    wram1: Box<[u8; WRAM1_LEN]>,
    wram2: Box<[u8; WRAM2_LEN]>,
    io_registers: IoRegisters,
    palette_ram: Box<[u8; PALETTE_RAM_LEN]>,
    vram: Box<[u8; VRAM_LEN]>,
    oam: Box<[u8; OAM_LEN]>,
    game_pak: Vec<u8>,
    game_pak_mask: usize,
    flash: FlashRegion,
    dma: [DmaChannel; 4],
    dma_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DmaTiming {
    Immediate,
    VBlank,
    HBlank,
    Special,
}

impl DmaTiming {
    fn decode(control: u16) -> DmaTiming {
        match (control >> 12) & 3 {
            0 => DmaTiming::Immediate,
            1 => DmaTiming::VBlank,
            2 => DmaTiming::HBlank,
            _ => DmaTiming::Special,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct DmaChannel {
    armed: bool,
    source: u32,
    destination: u32,
    count: u32,
}

impl Memory {
    pub fn new(bios: Vec<u8>, game_pak: Vec<u8>) -> Self {
        let mut bios_buffer = boxed_zeroed::<BIOS_LEN>();
        let bios_len = bios.len().min(BIOS_LEN);
        bios_buffer[..bios_len].copy_from_slice(&bios[..bios_len]);

        let rom_len = game_pak.len();
        let padded_len = rom_len.max(4).next_power_of_two();
        let mut game_pak = game_pak;
        game_pak.resize(padded_len, 0);
        let mut offset = (rom_len + 1) & !1;
        while offset < padded_len {
            write_halfword(&mut game_pak, offset, (offset >> 1) as u16);
            offset += 2;
        }

        Self {
            bios: bios_buffer,
            wram1: boxed_zeroed(),
            wram2: boxed_zeroed(),
            io_registers: IoRegisters::new(),
            palette_ram: boxed_zeroed(),
            vram: boxed_zeroed(),
            oam: boxed_zeroed(),
            game_pak,
            game_pak_mask: padded_len - 1,
            flash: FlashRegion::new(FLASH_LEN),
            dma: [DmaChannel::default(); 4],
            dma_active: false,
        }
    }

    #[inline(always)]
    fn decode_address(&self, address: u32) -> RegionKey {
        let offset = address as usize;
        match address >> 24 {
            0x00 => RegionKey::Bios(offset & (BIOS_LEN - 1)),
            0x02 => RegionKey::Wram1(offset & (WRAM1_LEN - 1)),
            0x03 => RegionKey::Wram2(offset & (WRAM2_LEN - 1)),
            0x04 => RegionKey::IoRegisters(address & 0x00FF_FFFF),
            0x05 => RegionKey::PaletteRam(offset & (PALETTE_RAM_LEN - 1)),
            0x06 => RegionKey::Vram(vram_offset(address)),
            0x07 => RegionKey::Oam(offset & (OAM_LEN - 1)),
            0x08..=0x0D => RegionKey::GamePak(offset & self.game_pak_mask),
            0x0E | 0x0F => RegionKey::Flash(address),
            _ => RegionKey::Unmapped,
        }
    }

    #[inline]
    pub fn read_u8(&self, address: u32) -> u8 {
        match self.decode_address(address) {
            RegionKey::Bios(offset) => self.bios[offset],
            RegionKey::Wram1(offset) => self.wram1[offset],
            RegionKey::Wram2(offset) => self.wram2[offset],
            RegionKey::IoRegisters(offset) => self.io_registers.read_u8(offset),
            RegionKey::PaletteRam(offset) => self.palette_ram[offset],
            RegionKey::Vram(offset) => self.vram[offset],
            RegionKey::Oam(offset) => self.oam[offset],
            RegionKey::GamePak(offset) => self.game_pak[offset],
            RegionKey::Flash(address) => self.flash.read_u8(address),
            RegionKey::Unmapped => 0,
        }
    }

    #[inline]
    pub fn read_u16(&self, address: u32) -> u16 {
        match self.decode_address(address & !0b1) {
            RegionKey::Bios(offset) => read_halfword(&self.bios[..], offset),
            RegionKey::Wram1(offset) => read_halfword(&self.wram1[..], offset),
            RegionKey::Wram2(offset) => read_halfword(&self.wram2[..], offset),
            RegionKey::IoRegisters(offset) => self.io_registers.read_u16(offset),
            RegionKey::PaletteRam(offset) => read_halfword(&self.palette_ram[..], offset),
            RegionKey::Vram(offset) => read_halfword(&self.vram[..], offset),
            RegionKey::Oam(offset) => read_halfword(&self.oam[..], offset),
            RegionKey::GamePak(offset) => read_halfword(&self.game_pak, offset),
            RegionKey::Flash(address) => self.flash.read_u8(address) as u16 * 0x0101,
            RegionKey::Unmapped => 0,
        }
    }

    #[inline]
    pub fn read_u32(&self, address: u32) -> u32 {
        match self.decode_address(address & !0b11) {
            RegionKey::Bios(offset) => read_word(&self.bios[..], offset),
            RegionKey::Wram1(offset) => read_word(&self.wram1[..], offset),
            RegionKey::Wram2(offset) => read_word(&self.wram2[..], offset),
            RegionKey::IoRegisters(offset) => self.io_registers.read_u32(offset),
            RegionKey::PaletteRam(offset) => read_word(&self.palette_ram[..], offset),
            RegionKey::Vram(offset) => read_word(&self.vram[..], offset),
            RegionKey::Oam(offset) => read_word(&self.oam[..], offset),
            RegionKey::GamePak(offset) => read_word(&self.game_pak, offset),
            RegionKey::Flash(address) => self.flash.read_u8(address) as u32 * 0x0101_0101,
            RegionKey::Unmapped => 0,
        }
    }

    #[inline]
    pub fn write_u8(&mut self, address: u32, value: u8) {
        match self.decode_address(address) {
            RegionKey::Wram1(offset) => self.wram1[offset] = value,
            RegionKey::Wram2(offset) => self.wram2[offset] = value,
            RegionKey::IoRegisters(offset) => self.io_registers.write_u8(offset, value),
            RegionKey::PaletteRam(offset) => write_halfword(&mut self.palette_ram[..], offset & !0b1, value as u16 * 0x0101),
            RegionKey::Vram(offset) => write_halfword(&mut self.vram[..], offset & !0b1, value as u16 * 0x0101),
            RegionKey::Flash(address) => self.flash.write_u8(address, value),
            RegionKey::Bios(_) | RegionKey::Oam(_) | RegionKey::GamePak(_) | RegionKey::Unmapped => {}
        }
    }

    #[inline]
    pub fn write_u16(&mut self, address: u32, value: u16) {
        match self.decode_address(address & !0b1) {
            RegionKey::Wram1(offset) => write_halfword(&mut self.wram1[..], offset, value),
            RegionKey::Wram2(offset) => write_halfword(&mut self.wram2[..], offset, value),
            RegionKey::IoRegisters(offset) => {
                self.io_registers.write_u16(offset, value);
                self.arm_dma_channels();
            }
            RegionKey::PaletteRam(offset) => write_halfword(&mut self.palette_ram[..], offset, value),
            RegionKey::Vram(offset) => write_halfword(&mut self.vram[..], offset, value),
            RegionKey::Oam(offset) => write_halfword(&mut self.oam[..], offset, value),
            RegionKey::Flash(address) => self.flash.write_u8(address, value as u8),
            RegionKey::Bios(_) | RegionKey::GamePak(_) | RegionKey::Unmapped => {}
        }
    }

    #[inline]
    pub fn write_u32(&mut self, address: u32, value: u32) {
        match self.decode_address(address & !0b11) {
            RegionKey::Wram1(offset) => write_word(&mut self.wram1[..], offset, value),
            RegionKey::Wram2(offset) => write_word(&mut self.wram2[..], offset, value),
            RegionKey::IoRegisters(offset) => {
                self.io_registers.write_u32(offset, value);
                self.arm_dma_channels();
            }
            RegionKey::PaletteRam(offset) => write_word(&mut self.palette_ram[..], offset, value),
            RegionKey::Vram(offset) => write_word(&mut self.vram[..], offset, value),
            RegionKey::Oam(offset) => write_word(&mut self.oam[..], offset, value),
            RegionKey::Flash(address) => self.flash.write_u8(address, value as u8),
            RegionKey::Bios(_) | RegionKey::GamePak(_) | RegionKey::Unmapped => {}
        }
    }

    pub fn print_io_registers(&self) {
        println!("{}", self.io_registers);
    }

    #[inline(always)]
    pub fn get_io_registers(&self) -> &IoRegisters {
        &self.io_registers
    }

    #[inline(always)]
    pub fn get_io_registers_mut(&mut self) -> &mut IoRegisters {
        &mut self.io_registers
    }

    pub fn vram(&self) -> &[u8; VRAM_LEN] {
        &self.vram
    }

    pub fn palette_ram(&self) -> &[u8; PALETTE_RAM_LEN] {
        &self.palette_ram
    }

    pub fn oam(&self) -> &[u8; OAM_LEN] {
        &self.oam
    }

    fn dma_length(&self, channel: usize) -> u32 {
        let count = self.io_registers.dma_cnt_l[channel] as u32;
        match (channel, count) {
            (3, 0) => 0x10000,
            (_, 0) => 0x4000,
            (3, count) => count,
            (_, count) => count & 0x3FFF,
        }
    }

    fn arm_dma_channels(&mut self) {
        for channel in 0..4 {
            let control = self.io_registers.dma_cnt_h[channel];
            let enabled = control & 0x8000 != 0;
            if enabled && !self.dma[channel].armed {
                self.dma[channel] = DmaChannel {
                    armed: true,
                    source: self.io_registers.dma_sad[channel],
                    destination: self.io_registers.dma_dad[channel],
                    count: self.dma_length(channel),
                };
                if DmaTiming::decode(control) == DmaTiming::Immediate {
                    self.run_dma(channel);
                }
            } else if !enabled {
                self.dma[channel].armed = false;
            }
        }
    }

    pub fn start_dma(&mut self, timing: DmaTiming) {
        for channel in 0..4 {
            if self.dma[channel].armed && DmaTiming::decode(self.io_registers.dma_cnt_h[channel]) == timing {
                self.run_dma(channel);
            }
        }
    }

    fn run_dma(&mut self, channel: usize) {
        if self.dma_active {
            return;
        }
        self.dma_active = true;

        let control = self.io_registers.dma_cnt_h[channel];
        let unit_size = if control & 0x400 != 0 { 4 } else { 2 };
        let source_control = (control >> 7) & 3;
        let destination_control = (control >> 5) & 3;
        let DmaChannel { count, mut source, mut destination, .. } = self.dma[channel];

        for _ in 0..count {
            if unit_size == 4 {
                let value = self.read_u32(source);
                self.write_u32(destination, value);
            } else {
                let value = self.read_u16(source);
                self.write_u16(destination, value);
            }
            source = match source_control {
                1 => source.wrapping_sub(unit_size),
                2 => source,
                _ => source.wrapping_add(unit_size),
            };
            destination = match destination_control {
                1 => destination.wrapping_sub(unit_size),
                2 => destination,
                _ => destination.wrapping_add(unit_size),
            };
        }

        let repeats = control & 0x200 != 0 && DmaTiming::decode(control) != DmaTiming::Immediate;
        self.dma[channel] = DmaChannel {
            armed: repeats,
            source,
            destination: if destination_control == 3 { self.io_registers.dma_dad[channel] } else { destination },
            count: self.dma_length(channel),
        };
        if !repeats {
            self.io_registers.dma_cnt_h[channel] &= !0x8000;
        }
        if control & 0x4000 != 0 {
            self.io_registers.irf |= 1 << (8 + channel);
        }

        self.dma_active = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_decoding() {
        let mem = Memory::new(vec![0; BIOS_LEN], vec![0; 0x100]);
        assert_eq!(mem.decode_address(0x0000_3FFF), RegionKey::Bios(0x3FFF));
        assert_eq!(mem.decode_address(0x0200_0000), RegionKey::Wram1(0));
        assert_eq!(mem.decode_address(0x0204_0004), RegionKey::Wram1(4));
        assert_eq!(mem.decode_address(0x0400_0208), RegionKey::IoRegisters(0x208));
        assert_eq!(mem.decode_address(0x0601_8000), RegionKey::Vram(0x1_0000));
        assert_eq!(mem.decode_address(0x0A00_0010), RegionKey::GamePak(0x10));
        assert_eq!(mem.decode_address(0x0E00_5555), RegionKey::Flash(0x0E00_5555));
        assert_eq!(mem.decode_address(0x0100_0000), RegionKey::Unmapped);
    }

    #[test]
    fn test_region_mirrors() {
        let mut mem = Memory::new(vec![0; BIOS_LEN], vec![0; 0x100]);
        mem.write_u32(0x0200_0000, 0x1234_5678);
        assert_eq!(mem.read_u32(0x0204_0000), 0x1234_5678);
        mem.write_u16(0x0300_7FF8, 0xBEEF);
        assert_eq!(mem.read_u16(0x0300_FFF8), 0xBEEF);
        assert_eq!(mem.read_u8(0x0300_7FF9), 0xBE);
    }

    #[test]
    fn test_vram_wrapping() {
        let mut mem = Memory::new(vec![], vec![]);
        mem.write_u16(0x0600_0000, 0x1234);
        assert_eq!(mem.read_u16(0x0600_0000), 0x1234);
        mem.write_u16(0x0601_8000, 0x5678);
        assert_eq!(mem.read_u16(0x0601_0000), 0x5678);
        mem.write_u16(0x0602_0000, 0x9ABC);
        assert_eq!(mem.read_u16(0x0600_0000), 0x9ABC);
    }

    #[test]
    fn test_rom_mirrors_and_open_bus_padding() {
        let mut rom = vec![0u8; 0x100];
        rom[0] = 0xAA;
        let mem = Memory::new(vec![], rom);
        assert_eq!(mem.read_u8(0x0800_0000), 0xAA);
        assert_eq!(mem.read_u8(0x0A00_0000), 0xAA);
        assert_eq!(mem.read_u8(0x0800_0100), 0xAA);
        let mem = Memory::new(vec![], vec![0; 0x102]);
        assert_eq!(mem.read_u16(0x0800_0102), 0x81);
    }

    #[test]
    fn test_unaligned_accesses_are_forced_aligned() {
        let mut mem = Memory::new(vec![], vec![]);
        mem.write_u32(0x0300_0000, 0x0403_0201);
        assert_eq!(mem.read_u32(0x0300_0002), 0x0403_0201);
        assert_eq!(mem.read_u16(0x0300_0003), 0x0403);
    }

    #[test]
    fn test_byte_writes_to_palette_and_vram_duplicate() {
        let mut mem = Memory::new(vec![], vec![]);
        mem.write_u8(0x0500_0001, 0x7F);
        assert_eq!(mem.read_u16(0x0500_0000), 0x7F7F);
        mem.write_u8(0x0600_0002, 0x12);
        assert_eq!(mem.read_u16(0x0600_0002), 0x1212);
        mem.write_u8(0x0700_0000, 0x12);
        assert_eq!(mem.read_u16(0x0700_0000), 0);
    }

    #[test]
    fn test_io_registers() {
        let mut io = IoRegisters::new();
        io.write_u16(0x000, 0x1234);
        assert_eq!(io.read_u16(0x000), 0x1234);
        assert_eq!(io.read_u8(0x001), 0x12);
        io.write_u32(0x008, 0x5678_9ABC);
        assert_eq!(io.read_u16(0x008), 0x9ABC);
        assert_eq!(io.read_u16(0x00A), 0x5678);
    }

    #[test]
    fn test_dispstat_flags_are_read_only() {
        let mut io = IoRegisters::new();
        io.disp_stat = 0x0003;
        io.write_u16(0x004, 0xFFFC);
        assert_eq!(io.read_u16(0x004), 0xFFFB);
        io.write_u16(0x004, 0x0000);
        assert_eq!(io.read_u16(0x004), 0x0003);
    }

    #[test]
    fn test_bg_reference_write_sets_flag() {
        let mut io = IoRegisters::new();
        io.write_u32(0x02C, 0x0123_4567);
        assert_eq!(io.bg_reference[0][1], 0x0123_4567);
        assert!(io.bg_reference_written[0]);
        assert!(!io.bg_reference_written[1]);
    }

    #[test]
    fn test_halt_control() {
        let mut io = IoRegisters::new();
        io.write_u8(0x301, 0);
        assert!(io.halted);
        io.halted = false;
        io.write_u8(0x300, 1);
        assert!(!io.halted);
        assert_eq!(io.read_u16(0x300), 1);
    }

    #[test]
    fn test_unmapped_reads_are_open_bus() {
        let mem = Memory::new(vec![], vec![]);
        assert_eq!(mem.read_u32(0x1000_0000), 0);
        assert_eq!(mem.read_u16(0x0100_0000), 0);
    }
}
