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

use super::{
    apu::{Apu, FIFO_A, FIFO_B},
    rtc::Gpio,
    save::{Backup, SaveType},
    state::{Reader, StateError, Writer},
};

const APU_REGISTERS: std::ops::Range<u32> = 0x060..0x0A8;

pub const BIOS_LEN: usize = 0x4000;
pub const WRAM1_LEN: usize = 0x4_0000;
pub const WRAM2_LEN: usize = 0x8000;
pub const PALETTE_RAM_LEN: usize = 0x400;
pub const VRAM_LEN: usize = 0x1_8000;
pub const OAM_LEN: usize = 0x400;
const GAME_PAK_MASK: usize = 0x01FF_FFFF;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    A,
    B,
    Select,
    Start,
    Right,
    Left,
    Up,
    Down,
    R,
    L,
}

impl Key {
    pub const ALL: [Key; 10] = [Key::A, Key::B, Key::Select, Key::Start, Key::Right, Key::Left, Key::Up, Key::Down, Key::R, Key::L];

    pub const fn bit(self) -> u16 {
        1 << self as u16
    }
}

pub struct IoRegisters {
    pub disp_cnt: u16,
    pub green_swap: u16,
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
    pub dma_sad: [u32; 4],
    pub dma_dad: [u32; 4],
    pub dma_cnt_l: [u16; 4],
    pub dma_cnt_h: [u16; 4],
    tm_reload: [u16; 4],
    tm_control: [u16; 4],
    tm_counter: [u16; 4],
    tm_cycles: [u32; 4],
    timer_overflows: u8,
    timer_pending: u32,
    timer_budget: u32,
    sio_data32: u32,
    sio_multi2: u16,
    sio_multi3: u16,
    sio_cnt: u16,
    sio_data8: u16,
    key_input: u16,
    key_cnt: u16,
    rcnt: u16,
    joy_cnt: u16,
    joy_recv: u32,
    joy_trans: u32,
    joy_stat: u16,
    pub ie: u16,
    pub irf: u16,
    pub wait_cnt: u16,
    pub ime: bool,
    post_flg: bool,
    pub halted: bool,
}

impl IoRegisters {
    pub fn new() -> Self {
        Self {
            disp_cnt: 0,
            green_swap: 0,
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
            dma_sad: [0; 4],
            dma_dad: [0; 4],
            dma_cnt_l: [0; 4],
            dma_cnt_h: [0; 4],
            tm_reload: [0; 4],
            tm_control: [0; 4],
            tm_counter: [0; 4],
            tm_cycles: [0; 4],
            timer_overflows: 0,
            timer_pending: 0,
            timer_budget: 0,
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
            0x002 => self.green_swap,
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
            0x0B8 => self.dma_cnt_l[0],
            0x0BA => self.dma_cnt_h[0],
            0x0C4 => self.dma_cnt_l[1],
            0x0C6 => self.dma_cnt_h[1],
            0x0D0 => self.dma_cnt_l[2],
            0x0D2 => self.dma_cnt_h[2],
            0x0DC => self.dma_cnt_l[3],
            0x0DE => self.dma_cnt_h[3],
            0x100 => self.timer_counter_now(0),
            0x102 => self.tm_control[0],
            0x104 => self.timer_counter_now(1),
            0x106 => self.tm_control[1],
            0x108 => self.timer_counter_now(2),
            0x10A => self.tm_control[2],
            0x10C => self.timer_counter_now(3),
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
            0x002 => self.green_swap = value & 1,
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
            0x100..=0x10E => self.write_timer_register(offset, value),
            0x120 => self.sio_data32 = self.sio_data32 & 0xFFFF_0000 | value as u32,
            0x122 => self.sio_data32 = self.sio_data32 & 0x0000_FFFF | (value as u32) << 16,
            0x124 => self.sio_multi2 = value,
            0x126 => self.sio_multi3 = value,
            0x128 => self.sio_cnt = value,
            0x12A => self.sio_data8 = value,
            0x132 => {
                self.key_cnt = value;
                self.check_key_interrupt();
            }
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

    pub fn set_pressed_keys(&mut self, pressed: u16) {
        self.key_input = !pressed & 0x03FF;
        self.check_key_interrupt();
    }

    pub fn pressed_keys(&self) -> u16 {
        !self.key_input & 0x03FF
    }

    fn check_key_interrupt(&mut self) {
        let selected = self.key_cnt & 0x03FF;
        let pressed = self.pressed_keys() & selected;
        let irq_enabled = self.key_cnt & 0x4000 != 0;
        let all_selected_required = self.key_cnt & 0x8000 != 0;
        let condition = if all_selected_required { selected != 0 && pressed == selected } else { pressed != 0 };
        if irq_enabled && condition {
            self.irf |= 1 << 12;
        }
    }

    pub fn save_state(&self, writer: &mut Writer) {
        writer.u16(self.disp_cnt);
        writer.u16(self.green_swap);
        writer.u16(self.disp_stat);
        writer.u16(self.v_count);
        writer.u16s(&self.bg_cnt);
        writer.u16s(&self.bg_h_offset);
        writer.u16s(&self.bg_v_offset);
        for parameters in &self.bg_parameters {
            writer.u16s(parameters);
        }
        for reference in &self.bg_reference {
            writer.u32s(reference);
        }
        writer.bools(&self.bg_reference_written);
        writer.u16s(&self.win_h);
        writer.u16s(&self.win_v);
        writer.u16(self.win_in);
        writer.u16(self.win_out);
        writer.u16(self.mosaic);
        writer.u16(self.blend_cnt);
        writer.u16(self.blend_alpha);
        writer.u16(self.blend_y);
        writer.u32s(&self.dma_sad);
        writer.u32s(&self.dma_dad);
        writer.u16s(&self.dma_cnt_l);
        writer.u16s(&self.dma_cnt_h);
        writer.u16s(&self.tm_reload);
        writer.u16s(&self.tm_control);
        writer.u16s(&self.tm_counter);
        writer.u32s(&self.tm_cycles);
        writer.u8(self.timer_overflows);
        writer.u32(self.timer_pending);
        writer.u32(self.sio_data32);
        writer.u16(self.sio_multi2);
        writer.u16(self.sio_multi3);
        writer.u16(self.sio_cnt);
        writer.u16(self.sio_data8);
        writer.u16(self.key_input);
        writer.u16(self.key_cnt);
        writer.u16(self.rcnt);
        writer.u16(self.joy_cnt);
        writer.u32(self.joy_recv);
        writer.u32(self.joy_trans);
        writer.u16(self.joy_stat);
        writer.u16(self.ie);
        writer.u16(self.irf);
        writer.u16(self.wait_cnt);
        writer.bool(self.ime);
        writer.bool(self.post_flg);
        writer.bool(self.halted);
    }

    pub fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.disp_cnt = reader.u16()?;
        self.green_swap = reader.u16()?;
        self.disp_stat = reader.u16()?;
        self.v_count = reader.u16()?;
        reader.u16s(&mut self.bg_cnt)?;
        reader.u16s(&mut self.bg_h_offset)?;
        reader.u16s(&mut self.bg_v_offset)?;
        for parameters in &mut self.bg_parameters {
            reader.u16s(parameters)?;
        }
        for reference in &mut self.bg_reference {
            reader.u32s(reference)?;
        }
        reader.bools(&mut self.bg_reference_written)?;
        reader.u16s(&mut self.win_h)?;
        reader.u16s(&mut self.win_v)?;
        self.win_in = reader.u16()?;
        self.win_out = reader.u16()?;
        self.mosaic = reader.u16()?;
        self.blend_cnt = reader.u16()?;
        self.blend_alpha = reader.u16()?;
        self.blend_y = reader.u16()?;
        reader.u32s(&mut self.dma_sad)?;
        reader.u32s(&mut self.dma_dad)?;
        reader.u16s(&mut self.dma_cnt_l)?;
        reader.u16s(&mut self.dma_cnt_h)?;
        reader.u16s(&mut self.tm_reload)?;
        reader.u16s(&mut self.tm_control)?;
        reader.u16s(&mut self.tm_counter)?;
        reader.u32s(&mut self.tm_cycles)?;
        self.timer_overflows = reader.u8()?;
        self.timer_pending = reader.u32()?;
        self.timer_budget = 0;
        self.sio_data32 = reader.u32()?;
        self.sio_multi2 = reader.u16()?;
        self.sio_multi3 = reader.u16()?;
        self.sio_cnt = reader.u16()?;
        self.sio_data8 = reader.u16()?;
        self.key_input = reader.u16()?;
        self.key_cnt = reader.u16()?;
        self.rcnt = reader.u16()?;
        self.joy_cnt = reader.u16()?;
        self.joy_recv = reader.u32()?;
        self.joy_trans = reader.u32()?;
        self.joy_stat = reader.u16()?;
        self.ie = reader.u16()?;
        self.irf = reader.u16()?;
        self.wait_cnt = reader.u16()?;
        self.ime = reader.bool()?;
        self.post_flg = reader.bool()?;
        self.halted = reader.bool()?;
        Ok(())
    }

    fn write_bg_reference(&mut self, bg: usize, axis: usize, value: u16, high: bool) {
        let reference = &mut self.bg_reference[bg][axis];
        *reference = if high {
            *reference & 0x0000_FFFF | (value as u32) << 16
        } else {
            *reference & 0xFFFF_0000 | value as u32
        };
        self.bg_reference_written[bg] = true;
    }

    fn write_timer_register(&mut self, offset: u32, value: u16) {
        self.flush_timers();
        let channel = ((offset - 0x100) / 4) as usize;
        if offset & 2 == 0 {
            self.tm_reload[channel] = value;
        } else {
            if value & 0x80 != 0 && self.tm_control[channel] & 0x80 == 0 {
                self.tm_counter[channel] = self.tm_reload[channel];
                self.tm_cycles[channel] = 0;
            }
            self.tm_control[channel] = value;
        }
        self.timer_budget = self.cycles_until_next_overflow();
    }

    fn timer_shift(control: u16) -> u32 {
        match control & 3 {
            0 => 0,
            1 => 6,
            2 => 8,
            _ => 10,
        }
    }

    fn timer_counts(&self, channel: usize) -> bool {
        let control = self.tm_control[channel];
        control & 0x80 != 0 && (channel == 0 || control & 0x4 == 0)
    }

    fn timer_counter_now(&self, channel: usize) -> u16 {
        if self.timer_counts(channel) {
            let elapsed = (self.tm_cycles[channel] + self.timer_pending) >> Self::timer_shift(self.tm_control[channel]);
            self.tm_counter[channel].wrapping_add(elapsed as u16)
        } else {
            self.tm_counter[channel]
        }
    }

    #[inline(always)]
    pub fn tick_timers(&mut self, cycles: u32) -> u8 {
        self.timer_pending += cycles;
        if self.timer_pending < self.timer_budget {
            0
        } else {
            self.flush_timers()
        }
    }

    pub fn cycles_until_timer_flush(&self) -> u32 {
        self.timer_budget.saturating_sub(self.timer_pending).max(1)
    }

    fn flush_timers(&mut self) -> u8 {
        let cycles = std::mem::replace(&mut self.timer_pending, 0);
        for channel in 0..4 {
            if self.timer_counts(channel) {
                let shift = Self::timer_shift(self.tm_control[channel]);
                let accumulated = self.tm_cycles[channel] + cycles;
                self.tm_cycles[channel] = accumulated & ((1 << shift) - 1);
                self.increment_timer(channel, accumulated >> shift);
            }
        }
        self.timer_budget = self.cycles_until_next_overflow();
        std::mem::replace(&mut self.timer_overflows, 0)
    }

    fn cycles_until_next_overflow(&self) -> u32 {
        let mut distance = 1 << 16;
        for channel in 0..4 {
            if self.timer_counts(channel) {
                let shift = Self::timer_shift(self.tm_control[channel]);
                let remaining = ((0x10000 - self.tm_counter[channel] as u32) << shift) - self.tm_cycles[channel];
                distance = distance.min(remaining);
            }
        }
        distance.max(1)
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
        self.timer_overflows |= 1 << channel;
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
pub enum Access {
    Nonsequential,
    Sequential,
}

#[derive(Debug, Clone, Copy)]
struct WaitStates {
    rom_nonsequential: [u32; 3],
    rom_sequential: [u32; 3],
    sram: u32,
    prefetch: bool,
}

impl WaitStates {
    fn decode(wait_cnt: u16) -> WaitStates {
        let first = |bits: u16| match bits & 0b11 {
            0 => 5,
            1 => 4,
            2 => 3,
            _ => 9,
        };
        let second = |bit: u16, slow: u32| if bit & 1 != 0 { 2 } else { slow };
        WaitStates {
            rom_nonsequential: [first(wait_cnt >> 2), first(wait_cnt >> 5), first(wait_cnt >> 8)],
            rom_sequential: [second(wait_cnt >> 4, 3), second(wait_cnt >> 7, 5), second(wait_cnt >> 10, 9)],
            sram: first(wait_cnt),
            prefetch: wait_cnt & 0x4000 != 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Prefetch {
    active: bool,
    start: u32,
    buffered: u32,
    progress: u32,
}

const PREFETCH_CAPACITY: u32 = 8;

fn is_rom(address: u32) -> bool {
    (0x08..=0x0D).contains(&(address >> 24))
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
    Gpio(u32),
    Eeprom,
    Backup(u32),
    Unmapped,
}

pub struct Memory {
    bios: Box<[u8; BIOS_LEN]>,
    wram1: Box<[u8; WRAM1_LEN]>,
    wram2: Box<[u8; WRAM2_LEN]>,
    io_registers: IoRegisters,
    pub apu: Apu,
    palette_ram: Box<[u8; PALETTE_RAM_LEN]>,
    vram: Box<[u8; VRAM_LEN]>,
    oam: Box<[u8; OAM_LEN]>,
    game_pak: Vec<u8>,
    game_pak_hash: u64,
    backup: Backup,
    gpio: Gpio,
    dma: [DmaChannel; 4],
    dma_active: bool,
    bios_last_opcode: u32,
    last_opcode: u32,
    executing_from_bios: bool,
    wait: WaitStates,
    prefetch: Prefetch,
    cycles: u32,
    next_fetch_sequential: bool,
    next_fetch_address: u32,
    apu_pending: u32,
}

const APU_BATCH_CYCLES: u32 = 512;

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

        let backup = Backup::new(SaveType::detect(&game_pak));
        let game_pak_hash = game_pak.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ *byte as u64).wrapping_mul(0x0000_0100_0000_01b3));

        Self {
            bios: bios_buffer,
            wram1: boxed_zeroed(),
            wram2: boxed_zeroed(),
            io_registers: IoRegisters::new(),
            apu: Apu::new(),
            palette_ram: boxed_zeroed(),
            vram: boxed_zeroed(),
            oam: boxed_zeroed(),
            game_pak,
            game_pak_hash,
            backup,
            gpio: Gpio::new(),
            dma: [DmaChannel::default(); 4],
            dma_active: false,
            bios_last_opcode: 0,
            last_opcode: 0,
            executing_from_bios: true,
            wait: WaitStates::decode(0),
            prefetch: Prefetch::default(),
            cycles: 0,
            next_fetch_sequential: false,
            next_fetch_address: 0,
            apu_pending: 0,
        }
    }

    #[inline(always)]
    pub fn take_cycles(&mut self) -> u32 {
        std::mem::replace(&mut self.cycles, 0)
    }

    #[inline(always)]
    pub fn idle(&mut self, cycles: u32) {
        self.cycles += cycles;
        self.advance_prefetch(cycles);
    }

    #[inline(always)]
    pub fn invalidate_fetch_sequence(&mut self) {
        self.next_fetch_sequential = false;
    }

    fn access_cycles(&self, address: u32, bytes: u32, access: Access) -> u32 {
        let word = bytes == 4;
        match address >> 24 {
            0x02 => {
                if word {
                    6
                } else {
                    3
                }
            }
            0x05 | 0x06 => {
                if word {
                    2
                } else {
                    1
                }
            }
            0x08..=0x0D => {
                let wait_state = ((address >> 25) - 4) as usize;
                let sequential = access == Access::Sequential && address & 0x1_FFFF != 0;
                let first = if sequential {
                    self.wait.rom_sequential[wait_state]
                } else {
                    self.wait.rom_nonsequential[wait_state]
                };
                if word {
                    first + self.wait.rom_sequential[wait_state]
                } else {
                    first
                }
            }
            0x0E | 0x0F => self.wait.sram,
            _ => 1,
        }
    }

    fn rom_sequential_cycles(&self, address: u32) -> u32 {
        self.wait.rom_sequential[((address >> 25) - 4) as usize]
    }

    fn advance_prefetch(&mut self, cycles: u32) {
        if self.prefetch.active && self.prefetch.buffered < PREFETCH_CAPACITY {
            let sequential = self.rom_sequential_cycles(self.prefetch.start);
            self.prefetch.progress += cycles;
            while self.prefetch.progress >= sequential && self.prefetch.buffered < PREFETCH_CAPACITY {
                self.prefetch.progress -= sequential;
                self.prefetch.buffered += 1;
            }
            if self.prefetch.buffered == PREFETCH_CAPACITY {
                self.prefetch.progress = 0;
            }
        }
    }

    fn prefetched_fetch_cycles(&mut self, address: u32, bytes: u32) -> u32 {
        let mut cycles = 0;
        for halfword in (address..address + bytes).step_by(2) {
            if self.prefetch.active && halfword == self.prefetch.start {
                if self.prefetch.buffered > 0 {
                    self.prefetch.buffered -= 1;
                    self.prefetch.start += 2;
                    cycles += 1;
                    self.advance_prefetch(1);
                } else {
                    let remaining = self.rom_sequential_cycles(halfword) - self.prefetch.progress;
                    self.prefetch.progress = 0;
                    self.prefetch.start += 2;
                    cycles += remaining;
                }
            } else {
                cycles += self.access_cycles(halfword, 2, Access::Nonsequential);
                self.prefetch = Prefetch {
                    active: true,
                    start: halfword + 2,
                    buffered: 0,
                    progress: 0,
                };
            }
        }
        cycles
    }

    #[inline]
    fn charge_fetch(&mut self, address: u32, bytes: u32) {
        let access = if self.next_fetch_sequential && address == self.next_fetch_address {
            Access::Sequential
        } else {
            Access::Nonsequential
        };
        let cycles = if is_rom(address) && self.wait.prefetch {
            self.prefetched_fetch_cycles(address, bytes)
        } else {
            if !is_rom(address) {
                self.prefetch.active = false;
            }
            self.access_cycles(address, bytes, access)
        };
        self.cycles += cycles;
        self.next_fetch_sequential = true;
        self.next_fetch_address = address.wrapping_add(bytes);
        self.executing_from_bios = address < BIOS_LEN as u32;
    }

    #[inline]
    fn charge_data_access(&mut self, address: u32, bytes: u32, access: Access) {
        let cycles = self.access_cycles(address, bytes, access);
        self.cycles += cycles;
        self.next_fetch_sequential = false;
        if is_rom(address) {
            self.prefetch.active = false;
        } else {
            self.advance_prefetch(cycles);
        }
    }

    #[inline]
    pub fn fetch_u32(&mut self, address: u32) -> u32 {
        self.charge_fetch(address, 4);
        let opcode = self.read_u32(address);
        if self.executing_from_bios {
            self.bios_last_opcode = opcode;
        }
        self.last_opcode = opcode;
        opcode
    }

    #[inline]
    pub fn fetch_u16(&mut self, address: u32) -> u16 {
        self.charge_fetch(address, 2);
        let opcode = self.read_u16(address);
        if self.executing_from_bios {
            self.bios_last_opcode = self.read_u32(address);
        }
        self.last_opcode = opcode as u32 | (opcode as u32) << 16;
        opcode
    }

    #[inline]
    pub fn load_u8(&mut self, address: u32, access: Access) -> u8 {
        self.charge_data_access(address, 1, access);
        self.read_u8(address)
    }

    #[inline]
    pub fn load_u16(&mut self, address: u32, access: Access) -> u16 {
        self.charge_data_access(address, 2, access);
        self.read_u16(address)
    }

    #[inline]
    pub fn load_u32(&mut self, address: u32, access: Access) -> u32 {
        self.charge_data_access(address, 4, access);
        self.read_u32(address)
    }

    #[inline]
    pub fn store_u8(&mut self, address: u32, value: u8, access: Access) {
        self.charge_data_access(address, 1, access);
        self.write_u8(address, value);
    }

    #[inline]
    pub fn store_u16(&mut self, address: u32, value: u16, access: Access) {
        self.charge_data_access(address, 2, access);
        self.write_u16(address, value);
    }

    #[inline]
    pub fn store_u32(&mut self, address: u32, value: u32, access: Access) {
        self.charge_data_access(address, 4, access);
        self.write_u32(address, value);
    }

    #[inline(always)]
    fn bios_read(&self, offset: usize) -> u32 {
        if self.executing_from_bios {
            read_word(&self.bios[..], offset & !0b11)
        } else {
            self.bios_last_opcode
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
            0x08 if (0x0800_00C4..0x0800_00CA).contains(&address) => RegionKey::Gpio(address - 0x0800_00C4),
            0x0D if self.backup.is_eeprom() && (self.game_pak.len() <= 0x100_0000 || address >= 0x0DFF_FF00) => RegionKey::Eeprom,
            0x08..=0x0D => RegionKey::GamePak(offset & GAME_PAK_MASK),
            0x0E | 0x0F => RegionKey::Backup(address),
            _ => RegionKey::Unmapped,
        }
    }

    #[inline]
    pub fn read_u8(&self, address: u32) -> u8 {
        match self.decode_address(address) {
            RegionKey::Bios(offset) => (self.bios_read(offset) >> (8 * (offset & 0b11))) as u8,
            RegionKey::Wram1(offset) => self.wram1[offset],
            RegionKey::Wram2(offset) => self.wram2[offset],
            RegionKey::IoRegisters(offset) => (self.io_read_u16(offset & !0b1) >> (8 * (offset & 1))) as u8,
            RegionKey::PaletteRam(offset) => self.palette_ram[offset],
            RegionKey::Vram(offset) => self.vram[offset],
            RegionKey::Oam(offset) => self.oam[offset],
            RegionKey::GamePak(offset) => self.rom_u8(offset),
            RegionKey::Gpio(offset) => (self.gpio_or_rom_u16(address & !0b1, offset & !0b1) >> (8 * (offset & 1))) as u8,
            RegionKey::Eeprom => self.backup.eeprom_read() as u8,
            RegionKey::Backup(address) => self.backup.read(address),
            RegionKey::Unmapped => (self.last_opcode >> (8 * (address & 0b11))) as u8,
        }
    }

    #[inline]
    pub fn read_u16(&self, address: u32) -> u16 {
        match self.decode_address(address & !0b1) {
            RegionKey::Bios(offset) => (self.bios_read(offset) >> (8 * (offset & 0b10))) as u16,
            RegionKey::Wram1(offset) => read_halfword(&self.wram1[..], offset),
            RegionKey::Wram2(offset) => read_halfword(&self.wram2[..], offset),
            RegionKey::IoRegisters(offset) => self.io_read_u16(offset),
            RegionKey::PaletteRam(offset) => read_halfword(&self.palette_ram[..], offset),
            RegionKey::Vram(offset) => read_halfword(&self.vram[..], offset),
            RegionKey::Oam(offset) => read_halfword(&self.oam[..], offset),
            RegionKey::GamePak(offset) => self.rom_u16(offset),
            RegionKey::Gpio(offset) => self.gpio_or_rom_u16(address & !0b1, offset),
            RegionKey::Eeprom => self.backup.eeprom_read(),
            RegionKey::Backup(address) => self.backup.read(address) as u16 * 0x0101,
            RegionKey::Unmapped => (self.last_opcode >> (8 * (address & 0b10))) as u16,
        }
    }

    #[inline]
    pub fn read_u32(&self, address: u32) -> u32 {
        match self.decode_address(address & !0b11) {
            RegionKey::Bios(offset) => self.bios_read(offset),
            RegionKey::Wram1(offset) => read_word(&self.wram1[..], offset),
            RegionKey::Wram2(offset) => read_word(&self.wram2[..], offset),
            RegionKey::IoRegisters(offset) => self.io_read_u16(offset) as u32 | (self.io_read_u16(offset + 2) as u32) << 16,
            RegionKey::PaletteRam(offset) => read_word(&self.palette_ram[..], offset),
            RegionKey::Vram(offset) => read_word(&self.vram[..], offset),
            RegionKey::Oam(offset) => read_word(&self.oam[..], offset),
            RegionKey::GamePak(offset) => self.rom_u32(offset),
            RegionKey::Gpio(offset) => self.gpio_or_rom_u16(address & !0b11, offset) as u32 | (self.gpio_or_rom_u16((address & !0b11) + 2, offset + 2) as u32) << 16,
            RegionKey::Eeprom => self.backup.eeprom_read() as u32 | (self.backup.eeprom_read() as u32) << 16,
            RegionKey::Backup(address) => self.backup.read(address) as u32 * 0x0101_0101,
            RegionKey::Unmapped => self.last_opcode,
        }
    }

    fn io_read_u16(&self, offset: u32) -> u16 {
        if APU_REGISTERS.contains(&offset) {
            self.apu.read_u16(offset)
        } else {
            self.io_registers.read_u16(offset)
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        let mut remaining = cycles;
        while remaining > 0 {
            let step = remaining.min(self.io_registers.cycles_until_timer_flush());
            remaining -= step;
            self.apu_pending += step;
            let overflowed = self.io_registers.tick_timers(step);
            if overflowed != 0 || self.apu_pending >= APU_BATCH_CYCLES {
                self.flush_apu();
            }
            for timer in 0..2u8 {
                if overflowed & (1 << timer) != 0 {
                    let refill = self.apu.timer_overflow(timer);
                    for (fifo, address) in [(0, FIFO_A), (1, FIFO_B)] {
                        if refill[fifo] {
                            self.refill_fifo(address);
                        }
                    }
                }
            }
        }
    }

    pub fn flush_apu(&mut self) {
        let cycles = std::mem::replace(&mut self.apu_pending, 0);
        if cycles > 0 {
            self.apu.run(cycles);
        }
    }

    fn refill_fifo(&mut self, fifo_address: u32) {
        for channel in 1..=2 {
            let control = self.io_registers.dma_cnt_h[channel];
            if self.dma[channel].armed && DmaTiming::decode(control) == DmaTiming::Special && self.io_registers.dma_dad[channel] == fifo_address {
                self.run_dma(channel);
            }
        }
    }

    fn gpio_or_rom_u16(&self, address: u32, offset: u32) -> u16 {
        if self.gpio.readable() && offset < 6 {
            self.gpio.read(offset)
        } else {
            self.rom_u16((address as usize) & GAME_PAK_MASK)
        }
    }

    #[inline(always)]
    fn rom_u8(&self, offset: usize) -> u8 {
        if offset < self.game_pak.len() {
            self.game_pak[offset]
        } else {
            ((offset >> 1) >> (8 * (offset & 1))) as u8
        }
    }

    #[inline(always)]
    fn rom_u16(&self, offset: usize) -> u16 {
        if offset + 2 <= self.game_pak.len() {
            read_halfword(&self.game_pak, offset)
        } else {
            self.rom_u8(offset) as u16 | (self.rom_u8(offset + 1) as u16) << 8
        }
    }

    #[inline(always)]
    fn rom_u32(&self, offset: usize) -> u32 {
        if offset + 4 <= self.game_pak.len() {
            read_word(&self.game_pak, offset)
        } else {
            self.rom_u16(offset) as u32 | (self.rom_u16(offset + 2) as u32) << 16
        }
    }

    #[inline]
    pub fn write_u8(&mut self, address: u32, value: u8) {
        match self.decode_address(address) {
            RegionKey::Wram1(offset) => self.wram1[offset] = value,
            RegionKey::Wram2(offset) => self.wram2[offset] = value,
            RegionKey::IoRegisters(offset) => {
                if APU_REGISTERS.contains(&offset) {
                    self.flush_apu();
                    let old = self.apu.read_u16(offset & !0b1);
                    let new = if offset & 1 == 0 { old & 0xFF00 | value as u16 } else { old & 0x00FF | (value as u16) << 8 };
                    self.apu.write_u16(offset & !0b1, new);
                } else {
                    self.io_registers.write_u8(offset, value);
                }
            }
            RegionKey::PaletteRam(offset) => write_halfword(&mut self.palette_ram[..], offset & !0b1, value as u16 * 0x0101),
            RegionKey::Vram(offset) => write_halfword(&mut self.vram[..], offset & !0b1, value as u16 * 0x0101),
            RegionKey::Backup(address) => self.backup.write(address, value),
            RegionKey::Bios(_) | RegionKey::Oam(_) | RegionKey::GamePak(_) | RegionKey::Gpio(_) | RegionKey::Eeprom | RegionKey::Unmapped => {}
        }
    }

    #[inline]
    pub fn write_u16(&mut self, address: u32, value: u16) {
        let selected_byte = (value >> (8 * (address & 0b1))) as u8;
        let aligned = address & !0b1;
        match self.decode_address(aligned) {
            RegionKey::Wram1(offset) => write_halfword(&mut self.wram1[..], offset, value),
            RegionKey::Wram2(offset) => write_halfword(&mut self.wram2[..], offset, value),
            RegionKey::IoRegisters(offset) => {
                if APU_REGISTERS.contains(&offset) {
                    self.flush_apu();
                    self.apu.write_u16(offset, value);
                } else {
                    self.io_registers.write_u16(offset, value);
                    self.after_io_write();
                }
            }
            RegionKey::PaletteRam(offset) => write_halfword(&mut self.palette_ram[..], offset, value),
            RegionKey::Vram(offset) => write_halfword(&mut self.vram[..], offset, value),
            RegionKey::Oam(offset) => write_halfword(&mut self.oam[..], offset, value),
            RegionKey::Gpio(offset) => self.gpio.write(offset, value),
            RegionKey::Eeprom => self.backup.eeprom_write(value),
            RegionKey::Backup(_) => self.backup.write(address, selected_byte),
            RegionKey::Bios(_) | RegionKey::GamePak(_) | RegionKey::Unmapped => {}
        }
    }

    #[inline]
    pub fn write_u32(&mut self, address: u32, value: u32) {
        let selected_byte = (value >> (8 * (address & 0b11))) as u8;
        let aligned = address & !0b11;
        match self.decode_address(aligned) {
            RegionKey::Wram1(offset) => write_word(&mut self.wram1[..], offset, value),
            RegionKey::Wram2(offset) => write_word(&mut self.wram2[..], offset, value),
            RegionKey::IoRegisters(offset) => match offset {
                0x0A0 => self.apu.write_fifo(0, value),
                0x0A4 => self.apu.write_fifo(1, value),
                _ if APU_REGISTERS.contains(&offset) => {
                    self.flush_apu();
                    self.apu.write_u16(offset, value as u16);
                    self.apu.write_u16(offset + 2, (value >> 16) as u16);
                }
                _ => {
                    self.io_registers.write_u32(offset, value);
                    self.after_io_write();
                }
            },
            RegionKey::PaletteRam(offset) => write_word(&mut self.palette_ram[..], offset, value),
            RegionKey::Vram(offset) => write_word(&mut self.vram[..], offset, value),
            RegionKey::Oam(offset) => write_word(&mut self.oam[..], offset, value),
            RegionKey::Gpio(offset) => {
                self.gpio.write(offset, value as u16);
                self.gpio.write(offset + 2, (value >> 16) as u16);
            }
            RegionKey::Eeprom => self.backup.eeprom_write(value as u16),
            RegionKey::Backup(_) => self.backup.write(address, selected_byte),
            RegionKey::Bios(_) | RegionKey::GamePak(_) | RegionKey::Unmapped => {}
        }
    }

    pub fn rom_identity(&self) -> Vec<u8> {
        let mut identity = (self.game_pak.len() as u64).to_le_bytes().to_vec();
        identity.extend_from_slice(&self.game_pak_hash.to_le_bytes());
        identity
    }

    pub fn save_state(&self, writer: &mut Writer) {
        writer.bytes(&self.wram1[..]);
        writer.bytes(&self.wram2[..]);
        writer.bytes(&self.palette_ram[..]);
        writer.bytes(&self.vram[..]);
        writer.bytes(&self.oam[..]);
        self.io_registers.save_state(writer);
        self.apu.save_state(writer);
        self.backup.save_state(writer);
        self.gpio.save_state(writer);
        for channel in &self.dma {
            writer.bool(channel.armed);
            writer.u32(channel.source);
            writer.u32(channel.destination);
            writer.u32(channel.count);
        }
        writer.bool(self.dma_active);
        writer.u32(self.bios_last_opcode);
        writer.u32(self.last_opcode);
        writer.bool(self.executing_from_bios);
        writer.bool(self.prefetch.active);
        writer.u32(self.prefetch.start);
        writer.u32(self.prefetch.buffered);
        writer.u32(self.prefetch.progress);
        writer.u32(self.cycles);
        writer.bool(self.next_fetch_sequential);
        writer.u32(self.next_fetch_address);
        writer.u32(self.apu_pending);
    }

    pub fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        reader.bytes_into(&mut self.wram1[..])?;
        reader.bytes_into(&mut self.wram2[..])?;
        reader.bytes_into(&mut self.palette_ram[..])?;
        reader.bytes_into(&mut self.vram[..])?;
        reader.bytes_into(&mut self.oam[..])?;
        self.io_registers.load_state(reader)?;
        self.wait = WaitStates::decode(self.io_registers.wait_cnt);
        self.apu.load_state(reader)?;
        self.backup.load_state(reader)?;
        self.gpio.load_state(reader)?;
        for channel in &mut self.dma {
            channel.armed = reader.bool()?;
            channel.source = reader.u32()?;
            channel.destination = reader.u32()?;
            channel.count = reader.u32()?;
        }
        self.dma_active = reader.bool()?;
        self.bios_last_opcode = reader.u32()?;
        self.last_opcode = reader.u32()?;
        self.executing_from_bios = reader.bool()?;
        self.prefetch.active = reader.bool()?;
        self.prefetch.start = reader.u32()?;
        self.prefetch.buffered = reader.u32()?;
        self.prefetch.progress = reader.u32()?;
        self.cycles = reader.u32()?;
        self.next_fetch_sequential = reader.bool()?;
        self.next_fetch_address = reader.u32()?;
        self.apu_pending = reader.u32()?;
        Ok(())
    }

    pub fn save_type(&self) -> SaveType {
        self.backup.save_type()
    }

    pub fn save_data(&self) -> &[u8] {
        self.backup.data()
    }

    pub fn load_save_data(&mut self, bytes: &[u8]) {
        self.backup.load(bytes);
    }

    pub fn take_save_dirty(&mut self) -> bool {
        self.backup.take_dirty()
    }

    pub fn set_time(&mut self, unix_seconds: u64) {
        self.gpio.rtc.set_time(unix_seconds);
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

    fn after_io_write(&mut self) {
        self.wait = WaitStates::decode(self.io_registers.wait_cnt);
        self.arm_dma_channels();
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
        let fifo_transfer = DmaTiming::decode(control) == DmaTiming::Special && (1..=2).contains(&channel);
        let unit_size = if control & 0x400 != 0 || fifo_transfer { 4 } else { 2 };
        let source_control = (control >> 7) & 3;
        let destination_control = if fifo_transfer { 2 } else { (control >> 5) & 3 };
        let DmaChannel {
            count, mut source, mut destination, ..
        } = self.dma[channel];
        let count = if fifo_transfer { 4 } else { count };
        if self.decode_address(destination) == RegionKey::Eeprom {
            self.backup.eeprom_begin_transfer(count);
        }
        let mut cycles = if is_rom(source) && is_rom(destination) { 4 } else { 2 };

        for i in 0..count {
            let access = if i == 0 { Access::Nonsequential } else { Access::Sequential };
            cycles += self.access_cycles(source, unit_size, access) + self.access_cycles(destination, unit_size, access);
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

        self.cycles += cycles;
        self.prefetch.active = false;
        self.next_fetch_sequential = false;
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
        assert_eq!(mem.decode_address(0x0E00_5555), RegionKey::Backup(0x0E00_5555));
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
    fn test_rom_wait_state_mirrors_and_open_bus() {
        let mut rom = vec![0u8; 0x100];
        rom[0] = 0xAA;
        let mem = Memory::new(vec![], rom);
        assert_eq!(mem.read_u8(0x0800_0000), 0xAA);
        assert_eq!(mem.read_u8(0x0A00_0000), 0xAA);
        assert_eq!(mem.read_u8(0x0C00_0000), 0xAA);
        assert_eq!(mem.read_u16(0x0800_0100), 0x0080);
        assert_eq!(mem.read_u8(0x0800_0201), 0x01);
        assert_eq!(mem.read_u32(0x0900_0000), 0x0001_0000);
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
    fn test_bios_reads_outside_bios_return_last_fetched_opcode() {
        let mut bios = vec![0u8; BIOS_LEN];
        bios[0x100..0x104].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        let mut mem = Memory::new(bios, vec![]);
        assert_eq!(mem.fetch_u32(0x100), 0x1234_5678);
        assert_eq!(mem.read_u32(0x200), 0);
        mem.fetch_u32(0x0800_0000);
        assert_eq!(mem.read_u32(0x200), 0x1234_5678);
        assert_eq!(mem.read_u16(0x202), 0x1234);
        assert_eq!(mem.read_u8(0x201), 0x56);
    }

    #[test]
    fn test_wait_state_decoding() {
        let default = WaitStates::decode(0);
        assert_eq!(default.rom_nonsequential, [5, 5, 5]);
        assert_eq!(default.rom_sequential, [3, 5, 9]);
        assert_eq!(default.sram, 5);
        assert!(!default.prefetch);
        let bios_setting = WaitStates::decode(0x4317);
        assert_eq!(bios_setting.rom_nonsequential[0], 4);
        assert_eq!(bios_setting.rom_sequential[0], 2);
        assert_eq!(bios_setting.sram, 9);
        assert!(bios_setting.prefetch);
    }

    #[test]
    fn test_access_cycles_per_region() {
        let mem = Memory::new(vec![], vec![0; 0x100]);
        assert_eq!(mem.access_cycles(0x0300_0000, 4, Access::Nonsequential), 1);
        assert_eq!(mem.access_cycles(0x0200_0000, 2, Access::Sequential), 3);
        assert_eq!(mem.access_cycles(0x0200_0000, 4, Access::Sequential), 6);
        assert_eq!(mem.access_cycles(0x0600_0000, 4, Access::Sequential), 2);
        assert_eq!(mem.access_cycles(0x0800_0000, 2, Access::Nonsequential), 5);
        assert_eq!(mem.access_cycles(0x0800_0002, 2, Access::Sequential), 3);
        assert_eq!(mem.access_cycles(0x0800_0000, 4, Access::Nonsequential), 8);
        assert_eq!(mem.access_cycles(0x0802_0000, 2, Access::Sequential), 5);
        assert_eq!(mem.access_cycles(0x0C00_0004, 4, Access::Sequential), 18);
        assert_eq!(mem.access_cycles(0x0E00_0000, 1, Access::Nonsequential), 5);
    }

    #[test]
    fn test_prefetch_buffer_serves_sequential_fetches() {
        let mut mem = Memory::new(vec![], vec![0; 0x100]);
        mem.write_u16(0x0400_0204, 0x4000);
        mem.fetch_u16(0x0800_0000);
        assert_eq!(mem.take_cycles(), 5);
        mem.fetch_u16(0x0800_0002);
        assert_eq!(mem.take_cycles(), 3);
        mem.idle(7);
        mem.take_cycles();
        mem.fetch_u16(0x0800_0004);
        assert_eq!(mem.take_cycles(), 1);
        mem.fetch_u16(0x0800_0006);
        assert_eq!(mem.take_cycles(), 1);
        mem.load_u32(0x0800_0040, Access::Nonsequential);
        mem.take_cycles();
        mem.fetch_u16(0x0800_0008);
        assert_eq!(mem.take_cycles(), 5);
    }

    #[test]
    fn test_unmapped_reads_return_the_last_prefetched_opcode() {
        let mut mem = Memory::new(vec![], vec![]);
        assert_eq!(mem.read_u32(0x1000_0000), 0);
        mem.write_u32(0x0300_0000, 0xE1A0_1234);
        mem.fetch_u32(0x0300_0000);
        assert_eq!(mem.read_u32(0x1000_0000), 0xE1A0_1234);
        assert_eq!(mem.read_u16(0x0100_0002), 0xE1A0);
        assert_eq!(mem.read_u8(0x0100_0001), 0x12);
        mem.write_u16(0x0300_0010, 0x46C0);
        mem.fetch_u16(0x0300_0010);
        assert_eq!(mem.read_u32(0x1000_0000), 0x46C0_46C0);
    }
}
