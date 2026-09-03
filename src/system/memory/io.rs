use crate::bits::Bits;

use crate::system::state::{Reader, StateError, Writer};

use super::{
    dma::{DmaControl, DmaRegisters},
    timers::Timers,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interrupt {
    VBlank,
    HBlank,
    VCount,
    Timer(usize),
    Serial,
    Dma(usize),
    Keypad,
    GamePak,
}

impl Interrupt {
    fn bit(self) -> u32 {
        match self {
            Interrupt::VBlank => 0,
            Interrupt::HBlank => 1,
            Interrupt::VCount => 2,
            Interrupt::Timer(index) => 3 + index as u32,
            Interrupt::Serial => 7,
            Interrupt::Dma(channel) => 8 + channel as u32,
            Interrupt::Keypad => 12,
            Interrupt::GamePak => 13,
        }
    }
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
    const MASK: u16 = 0x03FF;

    pub const fn number(self) -> u32 {
        self as u32
    }

    pub const fn bit(self) -> u16 {
        1 << self as u16
    }
}

#[derive(Debug, Default)]
struct Serial {
    data32: u32,
    multi: [u16; 2],
    control: u16,
    data8: u16,
    rcnt: u16,
    joy_control: u16,
    joy_receive: u32,
    joy_transmit: u32,
    joy_status: u16,
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
    pub dma: [DmaRegisters; 4],
    pub timers: Timers,
    serial: Serial,
    key_input: u16,
    key_cnt: u16,
    pub ie: u16,
    pub irf: u16,
    pub wait_cnt: u16,
    pub ime: bool,
    post_flg: bool,
    pub halted: bool,
}

impl IoRegisters {
    pub fn new() -> IoRegisters {
        IoRegisters {
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
            dma: [DmaRegisters::default(); 4],
            timers: Timers::default(),
            serial: Serial::default(),
            key_input: Key::MASK,
            key_cnt: 0,
            ie: 0,
            irf: 0,
            wait_cnt: 0,
            ime: false,
            post_flg: false,
            halted: false,
        }
    }

    pub fn read_u8(&self, offset: u32) -> u8 {
        self.read_u16(offset & !1).to_le_bytes()[(offset & 1) as usize]
    }

    pub fn read_u16(&self, offset: u32) -> u16 {
        let low = |word: u32| word as u16;
        let high = |word: u32| (word >> 16) as u16;
        match offset {
            0x000 => self.disp_cnt,
            0x002 => self.green_swap,
            0x004 => self.disp_stat,
            0x006 => self.v_count,
            0x008..=0x00E => self.bg_cnt[((offset - 0x008) / 2) as usize],
            0x048 => self.win_in,
            0x04A => self.win_out,
            0x050 => self.blend_cnt,
            0x052 => self.blend_alpha,
            0x0B0..=0x0DF => {
                let (channel, field) = ((offset - 0x0B0) / 12, (offset - 0x0B0) % 12);
                let dma = &self.dma[channel as usize];
                match field {
                    0 => low(dma.source),
                    2 => high(dma.source),
                    4 => low(dma.destination),
                    6 => high(dma.destination),
                    8 => dma.count,
                    _ => dma.control.0,
                }
            }
            0x100..=0x10E => self.timers.read(((offset - 0x100) / 4) as usize, offset & 2 != 0),
            0x120 => low(self.serial.data32),
            0x122 => high(self.serial.data32),
            0x124 => self.serial.multi[0],
            0x126 => self.serial.multi[1],
            0x128 => self.serial.control,
            0x12A => self.serial.data8,
            0x130 => self.key_input,
            0x132 => self.key_cnt,
            0x134 => self.serial.rcnt,
            0x140 => self.serial.joy_control,
            0x150 => low(self.serial.joy_receive),
            0x152 => high(self.serial.joy_receive),
            0x154 => low(self.serial.joy_transmit),
            0x156 => high(self.serial.joy_transmit),
            0x158 => self.serial.joy_status,
            0x200 => self.ie,
            0x202 => self.irf,
            0x204 => self.wait_cnt,
            0x208 => u16::from(self.ime),
            0x300 => u16::from(self.post_flg),
            _ => 0,
        }
    }

    pub fn read_u32(&self, offset: u32) -> u32 {
        u32::from(self.read_u16(offset)) | u32::from(self.read_u16(offset + 2)) << 16
    }

    pub fn write_u8(&mut self, offset: u32, value: u8) {
        match offset {
            0x202 | 0x203 => self.irf &= !(u16::from(value) << (8 * (offset & 1))),
            0x300 => self.post_flg = value.bit(0),
            0x301 => self.halted = !value.bit(7),
            _ => {
                let mut bytes = self.read_u16(offset & !1).to_le_bytes();
                bytes[(offset & 1) as usize] = value;
                self.write_u16(offset & !1, u16::from_le_bytes(bytes));
            }
        }
    }

    pub fn write_u16(&mut self, offset: u32, value: u16) {
        let with_low = |word: u32| word.with_bits(0..16, u32::from(value));
        let with_high = |word: u32| word.with_bits(16..32, u32::from(value));
        match offset {
            0x000 => self.disp_cnt = value,
            0x002 => self.green_swap = value & 1,
            0x004 => self.disp_stat = self.disp_stat.with_bits(3..16, value.bits(3..16)),
            0x008..=0x00E => self.bg_cnt[((offset - 0x008) / 2) as usize] = value,
            0x010..=0x01E => {
                let bg = ((offset - 0x010) / 4) as usize;
                if offset & 2 == 0 {
                    self.bg_h_offset[bg] = value.bits(0..9);
                } else {
                    self.bg_v_offset[bg] = value.bits(0..9);
                }
            }
            0x020..=0x03E => {
                let bg = ((offset - 0x020) / 16) as usize;
                match (offset % 16) / 2 {
                    parameter @ 0..=3 => self.bg_parameters[bg][parameter as usize] = value,
                    4 => self.bg_reference[bg][0] = with_low(self.bg_reference[bg][0]),
                    5 => self.bg_reference[bg][0] = with_high(self.bg_reference[bg][0]),
                    6 => self.bg_reference[bg][1] = with_low(self.bg_reference[bg][1]),
                    _ => self.bg_reference[bg][1] = with_high(self.bg_reference[bg][1]),
                }
                if offset % 16 >= 8 {
                    self.bg_reference_written[bg] = true;
                }
            }
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
            0x0B0..=0x0DF => {
                let (channel, field) = ((offset - 0x0B0) / 12, (offset - 0x0B0) % 12);
                let dma = &mut self.dma[channel as usize];
                match field {
                    0 => dma.source = with_low(dma.source),
                    2 => dma.source = with_high(dma.source),
                    4 => dma.destination = with_low(dma.destination),
                    6 => dma.destination = with_high(dma.destination),
                    8 => dma.count = value,
                    _ => dma.control = DmaControl(value),
                }
            }
            0x100..=0x10E => {
                let overflows = self.timers.write(((offset - 0x100) / 4) as usize, offset & 2 != 0, value);
                self.raise_timer_irqs(overflows);
            }
            0x120 => self.serial.data32 = with_low(self.serial.data32),
            0x122 => self.serial.data32 = with_high(self.serial.data32),
            0x124 => self.serial.multi[0] = value,
            0x126 => self.serial.multi[1] = value,
            0x128 => self.serial.control = value,
            0x12A => self.serial.data8 = value,
            0x132 => {
                self.key_cnt = value;
                self.check_key_interrupt();
            }
            0x134 => self.serial.rcnt = value,
            0x140 => self.serial.joy_control = value,
            0x150 => self.serial.joy_receive = with_low(self.serial.joy_receive),
            0x152 => self.serial.joy_receive = with_high(self.serial.joy_receive),
            0x154 => self.serial.joy_transmit = with_low(self.serial.joy_transmit),
            0x156 => self.serial.joy_transmit = with_high(self.serial.joy_transmit),
            0x158 => self.serial.joy_status = value,
            0x200 => self.ie = value,
            0x202 => self.irf &= !value,
            0x204 => self.wait_cnt = value,
            0x208 => self.ime = value.bit(0),
            0x300 => {
                self.post_flg = value.bit(0);
                self.halted = !value.bit(15);
            }
            _ => {}
        }
    }

    pub fn write_u32(&mut self, offset: u32, value: u32) {
        self.write_u16(offset, value as u16);
        self.write_u16(offset + 2, (value >> 16) as u16);
    }

    pub fn set_pressed_keys(&mut self, pressed: u16) {
        self.key_input = !pressed & Key::MASK;
        self.check_key_interrupt();
    }

    pub fn pressed_keys(&self) -> u16 {
        !self.key_input & Key::MASK
    }

    fn check_key_interrupt(&mut self) {
        let selected = self.key_cnt & Key::MASK;
        let pressed = self.pressed_keys() & selected;
        let irq_enabled = self.key_cnt.bit(14);
        let all_selected_required = self.key_cnt.bit(15);
        let condition = if all_selected_required { selected != 0 && pressed == selected } else { pressed != 0 };
        if irq_enabled && condition {
            self.raise(Interrupt::Keypad);
        }
    }

    pub fn raise(&mut self, interrupt: Interrupt) {
        self.irf = self.irf.with_bit(interrupt.bit(), true);
    }

    pub fn tick_timers(&mut self, cycles: u32) -> u8 {
        let overflows = self.timers.tick(cycles);
        self.raise_timer_irqs(overflows);
        overflows
    }

    fn raise_timer_irqs(&mut self, overflows: u8) {
        let requests = self.timers.irq_mask(overflows);
        for index in (0..4).filter(|index| requests.bit(*index as u32)) {
            self.raise(Interrupt::Timer(index));
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
        for dma in &self.dma {
            writer.u32(dma.source);
            writer.u32(dma.destination);
            writer.u16(dma.count);
            writer.u16(dma.control.0);
        }
        self.timers.save_state(writer);
        writer.u32(self.serial.data32);
        writer.u16s(&self.serial.multi);
        writer.u16(self.serial.control);
        writer.u16(self.serial.data8);
        writer.u16(self.key_input);
        writer.u16(self.key_cnt);
        writer.u16(self.serial.rcnt);
        writer.u16(self.serial.joy_control);
        writer.u32(self.serial.joy_receive);
        writer.u32(self.serial.joy_transmit);
        writer.u16(self.serial.joy_status);
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
        for dma in &mut self.dma {
            dma.source = reader.u32()?;
            dma.destination = reader.u32()?;
            dma.count = reader.u16()?;
            dma.control = DmaControl(reader.u16()?);
        }
        self.timers.load_state(reader)?;
        self.serial.data32 = reader.u32()?;
        reader.u16s(&mut self.serial.multi)?;
        self.serial.control = reader.u16()?;
        self.serial.data8 = reader.u16()?;
        self.key_input = reader.u16()?;
        self.key_cnt = reader.u16()?;
        self.serial.rcnt = reader.u16()?;
        self.serial.joy_control = reader.u16()?;
        self.serial.joy_receive = reader.u32()?;
        self.serial.joy_transmit = reader.u32()?;
        self.serial.joy_status = reader.u16()?;
        self.ie = reader.u16()?;
        self.irf = reader.u16()?;
        self.wait_cnt = reader.u16()?;
        self.ime = reader.bool()?;
        self.post_flg = reader.bool()?;
        self.halted = reader.bool()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_registers() {
        let mut io = IoRegisters::new();
        io.write_u16(0x000, 0x1234);
        assert_eq!(io.read_u16(0x000), 0x1234);
        assert_eq!(io.read_u8(0x001), 0x12);
        io.write_u32(0x008, 0x5678_9ABC);
        assert_eq!(io.read_u16(0x008), 0x9ABC);
        assert_eq!(io.read_u16(0x00A), 0x5678);
        io.write_u8(0x00B, 0x11);
        assert_eq!(io.read_u16(0x00A), 0x1178);
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
        io.write_u16(0x032, 0x0080);
        assert_eq!(io.bg_parameters[1][1], 0x0080);
        assert!(!io.bg_reference_written[1]);
    }

    #[test]
    fn test_dma_registers_are_indexed_by_channel() {
        let mut io = IoRegisters::new();
        io.write_u32(0x0D4, 0x0800_1234);
        io.write_u32(0x0D8, 0x0600_0000);
        io.write_u16(0x0DC, 0x100);
        io.write_u16(0x0DE, 0x8400);
        assert_eq!(io.dma[3].source, 0x0800_1234);
        assert_eq!(io.dma[3].destination, 0x0600_0000);
        assert_eq!(io.dma[3].count, 0x100);
        assert!(io.dma[3].control.enabled() && io.dma[3].control.transfers_words());
        assert_eq!(io.read_u16(0x0D6), 0x0800);
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
}
