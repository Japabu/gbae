mod bus;
mod dma;
mod io;
mod timers;

pub use bus::Access;
pub use dma::DmaTiming;
pub use io::{IoRegisters, Key};

use crate::bits::Bits;

use self::{
    bus::{is_rom, Bus},
    dma::{Adjust, DmaChannel},
};
use super::{
    apu::{Apu, FIFO_A, FIFO_B},
    bios::Bios,
    rtc::Gpio,
    save::{Backup, SaveType},
    state::{Reader, StateError, Writer},
};

pub const BIOS_LEN: usize = 0x4000;
pub const WRAM1_LEN: usize = 0x4_0000;
pub const WRAM2_LEN: usize = 0x8000;
pub const PALETTE_RAM_LEN: usize = 0x400;
pub const VRAM_LEN: usize = 0x1_8000;
pub const OAM_LEN: usize = 0x400;
const GAME_PAK_MASK: usize = 0x01FF_FFFF;
const APU_REGISTERS: std::ops::Range<u32> = 0x060..0x0A8;
const APU_BATCH_CYCLES: u32 = 512;
const FIFO_TRANSFER_WORDS: u32 = 4;

fn halfword(buffer: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([buffer[offset], buffer[offset + 1]])
}

fn word(buffer: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([buffer[offset], buffer[offset + 1], buffer[offset + 2], buffer[offset + 3]])
}

fn set_halfword(buffer: &mut [u8], offset: usize, value: u16) {
    buffer[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn set_word(buffer: &mut [u8], offset: usize, value: u32) {
    buffer[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn byte_of(value: u32, address: u32) -> u8 {
    value.to_le_bytes()[(address & 0b11) as usize]
}

fn halfword_of(value: u32, address: u32) -> u16 {
    if address.bit(1) {
        (value >> 16) as u16
    } else {
        value as u16
    }
}

fn boxed_zeroed<const N: usize>() -> Box<[u8; N]> {
    vec![0u8; N].into_boxed_slice().try_into().unwrap()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ram {
    Ewram,
    Iwram,
    Palette,
    Vram,
    Oam,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Region {
    Bios(usize),
    Ram(Ram, usize),
    Io(u32),
    Rom(usize),
    Gpio(u32),
    Eeprom,
    Backup(u32),
    Unmapped,
}

pub struct Memory {
    bios: Box<[u8; BIOS_LEN]>,
    ewram: Box<[u8; WRAM1_LEN]>,
    iwram: Box<[u8; WRAM2_LEN]>,
    palette: Box<[u8; PALETTE_RAM_LEN]>,
    vram: Box<[u8; VRAM_LEN]>,
    oam: Box<[u8; OAM_LEN]>,
    io: IoRegisters,
    pub apu: Apu,
    game_pak: Vec<u8>,
    game_pak_hash: u64,
    backup: Backup,
    gpio: Gpio,
    dma: [DmaChannel; 4],
    dma_active: bool,
    bus: Bus,
    bios_last_opcode: u32,
    last_opcode: u32,
    executing_from_bios: bool,
    builtin_bios: bool,
    intr_waiting: bool,
    apu_pending: u32,
}

impl Memory {
    pub fn new(bios: Bios, game_pak: Vec<u8>) -> Memory {
        let builtin_bios = bios.is_builtin();
        let image = bios.bytes();
        let mut bios = boxed_zeroed::<BIOS_LEN>();
        let length = image.len().min(BIOS_LEN);
        bios[..length].copy_from_slice(&image[..length]);
        let backup = Backup::new(SaveType::detect(&game_pak));
        let game_pak_hash = game_pak
            .iter()
            .fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3));
        Memory {
            bios,
            ewram: boxed_zeroed(),
            iwram: boxed_zeroed(),
            palette: boxed_zeroed(),
            vram: boxed_zeroed(),
            oam: boxed_zeroed(),
            io: IoRegisters::new(),
            apu: Apu::new(),
            game_pak,
            game_pak_hash,
            backup,
            gpio: Gpio::new(),
            dma: [DmaChannel::default(); 4],
            dma_active: false,
            bus: Bus::new(),
            bios_last_opcode: 0,
            last_opcode: 0,
            executing_from_bios: true,
            builtin_bios,
            intr_waiting: false,
            apu_pending: 0,
        }
    }

    pub fn has_builtin_bios(&self) -> bool {
        self.builtin_bios
    }

    pub fn intr_waiting(&self) -> bool {
        self.intr_waiting
    }

    pub fn set_intr_waiting(&mut self, waiting: bool) {
        self.intr_waiting = waiting;
    }

    pub fn io(&self) -> &IoRegisters {
        &self.io
    }

    pub fn io_mut(&mut self) -> &mut IoRegisters {
        &mut self.io
    }

    pub fn vram(&self) -> &[u8; VRAM_LEN] {
        &self.vram
    }

    pub fn palette_ram(&self) -> &[u8; PALETTE_RAM_LEN] {
        &self.palette
    }

    pub fn oam(&self) -> &[u8; OAM_LEN] {
        &self.oam
    }

    pub fn take_cycles(&mut self) -> u32 {
        self.bus.take_cycles()
    }

    pub fn idle(&mut self, cycles: u32) {
        self.bus.idle(cycles);
    }

    pub fn invalidate_fetch_sequence(&mut self) {
        self.bus.invalidate_sequence();
    }

    pub fn fetch_u32(&mut self, address: u32) -> u32 {
        self.bus.charge_fetch(address, 4);
        self.executing_from_bios = address < BIOS_LEN as u32;
        let opcode = self.read_u32(address);
        if self.executing_from_bios {
            self.bios_last_opcode = opcode;
        }
        self.last_opcode = opcode;
        opcode
    }

    pub fn fetch_u16(&mut self, address: u32) -> u16 {
        self.bus.charge_fetch(address, 2);
        self.executing_from_bios = address < BIOS_LEN as u32;
        let opcode = self.read_u16(address);
        if self.executing_from_bios {
            self.bios_last_opcode = self.read_u32(address);
        }
        self.last_opcode = u32::from(opcode) * 0x0001_0001;
        opcode
    }

    pub fn load_u8(&mut self, address: u32, access: Access) -> u8 {
        self.bus.charge_data(address, 1, access);
        self.read_u8(address)
    }

    pub fn load_u16(&mut self, address: u32, access: Access) -> u16 {
        self.bus.charge_data(address, 2, access);
        self.read_u16(address)
    }

    pub fn load_u32(&mut self, address: u32, access: Access) -> u32 {
        self.bus.charge_data(address, 4, access);
        self.read_u32(address)
    }

    pub fn store_u8(&mut self, address: u32, value: u8, access: Access) {
        self.bus.charge_data(address, 1, access);
        self.write_u8(address, value);
    }

    pub fn store_u16(&mut self, address: u32, value: u16, access: Access) {
        self.bus.charge_data(address, 2, access);
        self.write_u16(address, value);
    }

    pub fn store_u32(&mut self, address: u32, value: u32, access: Access) {
        self.bus.charge_data(address, 4, access);
        self.write_u32(address, value);
    }

    fn region(&self, address: u32) -> Region {
        let offset = address as usize;
        match address >> 24 {
            0x00 => Region::Bios(offset & (BIOS_LEN - 1)),
            0x02 => Region::Ram(Ram::Ewram, offset & (WRAM1_LEN - 1)),
            0x03 => Region::Ram(Ram::Iwram, offset & (WRAM2_LEN - 1)),
            0x04 => Region::Io(address.bits(0..24)),
            0x05 => Region::Ram(Ram::Palette, offset & (PALETTE_RAM_LEN - 1)),
            0x06 => Region::Ram(Ram::Vram, vram_offset(address)),
            0x07 => Region::Ram(Ram::Oam, offset & (OAM_LEN - 1)),
            0x08 if (0x0800_00C4..0x0800_00CA).contains(&address) => Region::Gpio(address - 0x0800_00C4),
            0x0D if self.backup.is_eeprom() && (self.game_pak.len() <= 0x100_0000 || address >= 0x0DFF_FF00) => Region::Eeprom,
            0x08..=0x0D => Region::Rom(offset & GAME_PAK_MASK),
            0x0E | 0x0F => Region::Backup(address),
            _ => Region::Unmapped,
        }
    }

    fn ram(&self, ram: Ram) -> &[u8] {
        match ram {
            Ram::Ewram => &self.ewram[..],
            Ram::Iwram => &self.iwram[..],
            Ram::Palette => &self.palette[..],
            Ram::Vram => &self.vram[..],
            Ram::Oam => &self.oam[..],
        }
    }

    fn ram_mut(&mut self, ram: Ram) -> &mut [u8] {
        match ram {
            Ram::Ewram => &mut self.ewram[..],
            Ram::Iwram => &mut self.iwram[..],
            Ram::Palette => &mut self.palette[..],
            Ram::Vram => &mut self.vram[..],
            Ram::Oam => &mut self.oam[..],
        }
    }

    fn bios_word(&self, offset: usize) -> u32 {
        if self.executing_from_bios {
            word(&self.bios[..], offset & !0b11)
        } else {
            self.bios_last_opcode
        }
    }

    fn io_read_u16(&self, offset: u32) -> u16 {
        if APU_REGISTERS.contains(&offset) {
            self.apu.read_u16(offset)
        } else {
            self.io.read_u16(offset)
        }
    }

    pub fn read_u8(&self, address: u32) -> u8 {
        match self.region(address) {
            Region::Bios(offset) => byte_of(self.bios_word(offset), address),
            Region::Ram(ram, offset) => self.ram(ram)[offset],
            Region::Io(offset) => self.io_read_u16(offset & !1).to_le_bytes()[(offset & 1) as usize],
            Region::Rom(offset) => self.rom_u8(offset),
            Region::Gpio(offset) => self.gpio_or_rom_u16(address & !1, offset & !1).to_le_bytes()[(offset & 1) as usize],
            Region::Eeprom => self.backup.eeprom_read() as u8,
            Region::Backup(address) => self.backup.read(address),
            Region::Unmapped => byte_of(self.last_opcode, address),
        }
    }

    pub fn read_u16(&self, address: u32) -> u16 {
        let address = address & !0b1;
        match self.region(address) {
            Region::Bios(offset) => halfword_of(self.bios_word(offset), address),
            Region::Ram(ram, offset) => halfword(self.ram(ram), offset),
            Region::Io(offset) => self.io_read_u16(offset),
            Region::Rom(offset) => self.rom_u16(offset),
            Region::Gpio(offset) => self.gpio_or_rom_u16(address, offset),
            Region::Eeprom => self.backup.eeprom_read(),
            Region::Backup(address) => u16::from(self.backup.read(address)) * 0x0101,
            Region::Unmapped => halfword_of(self.last_opcode, address),
        }
    }

    pub fn read_u32(&self, address: u32) -> u32 {
        let address = address & !0b11;
        match self.region(address) {
            Region::Bios(offset) => self.bios_word(offset),
            Region::Ram(ram, offset) => word(self.ram(ram), offset),
            Region::Io(offset) => u32::from(self.io_read_u16(offset)) | u32::from(self.io_read_u16(offset + 2)) << 16,
            Region::Rom(offset) => self.rom_u32(offset),
            Region::Gpio(offset) => u32::from(self.gpio_or_rom_u16(address, offset)) | u32::from(self.gpio_or_rom_u16(address + 2, offset + 2)) << 16,
            Region::Eeprom => u32::from(self.backup.eeprom_read()) | u32::from(self.backup.eeprom_read()) << 16,
            Region::Backup(address) => u32::from(self.backup.read(address)) * 0x0101_0101,
            Region::Unmapped => self.last_opcode,
        }
    }

    pub fn write_u8(&mut self, address: u32, value: u8) {
        match self.region(address) {
            Region::Ram(Ram::Oam, _) => {}
            Region::Ram(ram @ (Ram::Palette | Ram::Vram), offset) => set_halfword(self.ram_mut(ram), offset & !1, u16::from(value) * 0x0101),
            Region::Ram(ram, offset) => self.ram_mut(ram)[offset] = value,
            Region::Io(offset) if APU_REGISTERS.contains(&offset) => {
                self.flush_apu();
                let mut bytes = self.apu.read_u16(offset & !1).to_le_bytes();
                bytes[(offset & 1) as usize] = value;
                self.apu.write_u16(offset & !1, u16::from_le_bytes(bytes));
            }
            Region::Io(offset) => {
                self.io.write_u8(offset, value);
                self.after_io_write();
            }
            Region::Backup(address) => self.backup.write(address, value),
            Region::Bios(_) | Region::Rom(_) | Region::Gpio(_) | Region::Eeprom | Region::Unmapped => {}
        }
    }

    pub fn write_u16(&mut self, address: u32, value: u16) {
        let selected_byte = value.to_le_bytes()[(address & 0b1) as usize];
        match self.region(address & !0b1) {
            Region::Ram(ram, offset) => set_halfword(self.ram_mut(ram), offset, value),
            Region::Io(offset) if APU_REGISTERS.contains(&offset) => {
                self.flush_apu();
                self.apu.write_u16(offset, value);
            }
            Region::Io(offset) => {
                self.io.write_u16(offset, value);
                self.after_io_write();
            }
            Region::Gpio(offset) => self.gpio.write(offset, value),
            Region::Eeprom => self.backup.eeprom_write(value),
            Region::Backup(_) => self.backup.write(address, selected_byte),
            Region::Bios(_) | Region::Rom(_) | Region::Unmapped => {}
        }
    }

    pub fn write_u32(&mut self, address: u32, value: u32) {
        let selected_byte = byte_of(value, address);
        match self.region(address & !0b11) {
            Region::Ram(ram, offset) => set_word(self.ram_mut(ram), offset, value),
            Region::Io(0x0A0) => self.apu.write_fifo(0, value),
            Region::Io(0x0A4) => self.apu.write_fifo(1, value),
            Region::Io(offset) if APU_REGISTERS.contains(&offset) => {
                self.flush_apu();
                self.apu.write_u16(offset, value as u16);
                self.apu.write_u16(offset + 2, (value >> 16) as u16);
            }
            Region::Io(offset) => {
                self.io.write_u32(offset, value);
                self.after_io_write();
            }
            Region::Gpio(offset) => {
                self.gpio.write(offset, value as u16);
                self.gpio.write(offset + 2, (value >> 16) as u16);
            }
            Region::Eeprom => self.backup.eeprom_write(value as u16),
            Region::Backup(_) => self.backup.write(address, selected_byte),
            Region::Bios(_) | Region::Rom(_) | Region::Unmapped => {}
        }
    }

    fn gpio_or_rom_u16(&self, address: u32, offset: u32) -> u16 {
        if self.gpio.readable() && offset < 6 {
            self.gpio.read(offset)
        } else {
            self.rom_u16(address as usize & GAME_PAK_MASK)
        }
    }

    fn rom_u8(&self, offset: usize) -> u8 {
        match self.game_pak.get(offset) {
            Some(byte) => *byte,
            None => ((offset >> 1) as u16).to_le_bytes()[offset & 1],
        }
    }

    fn rom_u16(&self, offset: usize) -> u16 {
        if offset + 2 <= self.game_pak.len() {
            halfword(&self.game_pak, offset)
        } else {
            u16::from_le_bytes([self.rom_u8(offset), self.rom_u8(offset + 1)])
        }
    }

    fn rom_u32(&self, offset: usize) -> u32 {
        if offset + 4 <= self.game_pak.len() {
            word(&self.game_pak, offset)
        } else {
            u32::from(self.rom_u16(offset)) | u32::from(self.rom_u16(offset + 2)) << 16
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        let mut remaining = cycles;
        while remaining > 0 {
            let step = remaining.min(self.io.timers.cycles_until_flush());
            remaining -= step;
            self.apu_pending += step;
            let overflowed = self.io.tick_timers(step);
            if overflowed != 0 || self.apu_pending >= APU_BATCH_CYCLES {
                self.flush_apu();
            }
            for timer in (0..2u8).filter(|timer| overflowed.bit(u32::from(*timer))) {
                let refill = self.apu.timer_overflow(timer);
                for (address, needed) in [FIFO_A, FIFO_B].into_iter().zip(refill) {
                    if needed {
                        self.refill_fifo(address);
                    }
                }
            }
        }
    }

    pub fn flush_apu(&mut self) {
        let cycles = std::mem::take(&mut self.apu_pending);
        if cycles > 0 {
            self.apu.run(cycles);
        }
    }

    fn refill_fifo(&mut self, fifo_address: u32) {
        for channel in 1..=2 {
            let registers = self.io.dma[channel];
            if self.dma[channel].armed && registers.control.timing() == DmaTiming::Special && registers.destination == fifo_address {
                self.run_dma(channel);
            }
        }
    }

    fn after_io_write(&mut self) {
        self.bus.configure(self.io.wait_cnt);
        self.arm_dma_channels();
    }

    fn arm_dma_channels(&mut self) {
        for channel in 0..4 {
            let registers = self.io.dma[channel];
            if registers.control.enabled() && !self.dma[channel].armed {
                self.dma[channel] = DmaChannel {
                    armed: true,
                    source: registers.source,
                    destination: registers.destination,
                    count: registers.length(channel),
                };
                if registers.control.timing() == DmaTiming::Immediate {
                    self.run_dma(channel);
                }
            } else if !registers.control.enabled() {
                self.dma[channel].armed = false;
            }
        }
    }

    pub fn start_dma(&mut self, timing: DmaTiming) {
        for channel in 0..4 {
            if self.dma[channel].armed && self.io.dma[channel].control.timing() == timing {
                self.run_dma(channel);
            }
        }
    }

    fn run_dma(&mut self, channel: usize) {
        if self.dma_active {
            return;
        }
        self.dma_active = true;

        let registers = self.io.dma[channel];
        let control = registers.control;
        let fifo_transfer = control.timing() == DmaTiming::Special && (1..=2).contains(&channel);
        let unit = if control.transfers_words() || fifo_transfer { 4 } else { 2 };
        let source_adjust = control.source_adjust();
        let destination_adjust = if fifo_transfer { Adjust::Fixed } else { control.destination_adjust() };
        let DmaChannel {
            count, mut source, mut destination, ..
        } = self.dma[channel];
        let count = if fifo_transfer { FIFO_TRANSFER_WORDS } else { count };
        if self.region(destination) == Region::Eeprom {
            self.backup.eeprom_begin_transfer(count);
        }
        let mut cycles = if is_rom(source) && is_rom(destination) { 4 } else { 2 };

        for index in 0..count {
            let access = if index == 0 { Access::Nonsequential } else { Access::Sequential };
            cycles += self.bus.access_cycles(source, unit, access) + self.bus.access_cycles(destination, unit, access);
            if unit == 4 {
                let value = self.read_u32(source);
                self.write_u32(destination, value);
            } else {
                let value = self.read_u16(source);
                self.write_u16(destination, value);
            }
            source = source_adjust.apply(source, unit);
            destination = destination_adjust.apply(destination, unit);
        }

        let repeats = control.repeats() && control.timing() != DmaTiming::Immediate;
        self.dma[channel] = DmaChannel {
            armed: repeats,
            source,
            destination: if destination_adjust == Adjust::Reload { registers.destination } else { destination },
            count: registers.length(channel),
        };
        if !repeats {
            self.io.dma[channel].control = control.disabled();
        }
        if control.raises_irq() {
            self.io.irf |= 1 << (8 + channel);
        }

        self.bus.add_cycles(cycles);
        self.bus.interrupt();
        self.dma_active = false;
    }

    pub fn rom_identity(&self) -> Vec<u8> {
        let mut identity = (self.game_pak.len() as u64).to_le_bytes().to_vec();
        identity.extend_from_slice(&self.game_pak_hash.to_le_bytes());
        identity
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

    pub fn save_state(&self, writer: &mut Writer) {
        writer.bytes(&self.ewram[..]);
        writer.bytes(&self.iwram[..]);
        writer.bytes(&self.palette[..]);
        writer.bytes(&self.vram[..]);
        writer.bytes(&self.oam[..]);
        self.io.save_state(writer);
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
        self.bus.save_state(writer);
        writer.u32(self.apu_pending);
        writer.bool(self.intr_waiting);
    }

    pub fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        reader.bytes_into(&mut self.ewram[..])?;
        reader.bytes_into(&mut self.iwram[..])?;
        reader.bytes_into(&mut self.palette[..])?;
        reader.bytes_into(&mut self.vram[..])?;
        reader.bytes_into(&mut self.oam[..])?;
        self.io.load_state(reader)?;
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
        self.bus.load_state(reader, self.io.wait_cnt)?;
        self.apu_pending = reader.u32()?;
        self.intr_waiting = reader.bool()?;
        Ok(())
    }
}

fn vram_offset(address: u32) -> usize {
    let offset = address.bits(0..17) as usize;
    if offset >= VRAM_LEN {
        offset - 0x8000
    } else {
        offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(rom: Vec<u8>) -> Memory {
        Memory::new(Bios::Image(vec![0; BIOS_LEN]), rom)
    }

    #[test]
    fn test_address_decoding() {
        let mem = memory(vec![0; 0x100]);
        assert_eq!(mem.region(0x0000_3FFF), Region::Bios(0x3FFF));
        assert_eq!(mem.region(0x0200_0000), Region::Ram(Ram::Ewram, 0));
        assert_eq!(mem.region(0x0204_0004), Region::Ram(Ram::Ewram, 4));
        assert_eq!(mem.region(0x0400_0208), Region::Io(0x208));
        assert_eq!(mem.region(0x0601_8000), Region::Ram(Ram::Vram, 0x1_0000));
        assert_eq!(mem.region(0x0A00_0010), Region::Rom(0x10));
        assert_eq!(mem.region(0x0E00_5555), Region::Backup(0x0E00_5555));
        assert_eq!(mem.region(0x0100_0000), Region::Unmapped);
    }

    #[test]
    fn test_region_mirrors() {
        let mut mem = memory(vec![0; 0x100]);
        mem.write_u32(0x0200_0000, 0x1234_5678);
        assert_eq!(mem.read_u32(0x0204_0000), 0x1234_5678);
        mem.write_u16(0x0300_7FF8, 0xBEEF);
        assert_eq!(mem.read_u16(0x0300_FFF8), 0xBEEF);
        assert_eq!(mem.read_u8(0x0300_7FF9), 0xBE);
    }

    #[test]
    fn test_vram_wrapping() {
        let mut mem = memory(vec![]);
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
        let mem = memory(rom);
        assert_eq!(mem.read_u8(0x0800_0000), 0xAA);
        assert_eq!(mem.read_u8(0x0A00_0000), 0xAA);
        assert_eq!(mem.read_u8(0x0C00_0000), 0xAA);
        assert_eq!(mem.read_u16(0x0800_0100), 0x0080);
        assert_eq!(mem.read_u8(0x0800_0201), 0x01);
        assert_eq!(mem.read_u32(0x0900_0000), 0x0001_0000);
        let mem = memory(vec![0; 0x102]);
        assert_eq!(mem.read_u16(0x0800_0102), 0x81);
    }

    #[test]
    fn test_unaligned_accesses_are_forced_aligned() {
        let mut mem = memory(vec![]);
        mem.write_u32(0x0300_0000, 0x0403_0201);
        assert_eq!(mem.read_u32(0x0300_0002), 0x0403_0201);
        assert_eq!(mem.read_u16(0x0300_0003), 0x0403);
    }

    #[test]
    fn test_byte_writes_to_palette_and_vram_duplicate() {
        let mut mem = memory(vec![]);
        mem.write_u8(0x0500_0001, 0x7F);
        assert_eq!(mem.read_u16(0x0500_0000), 0x7F7F);
        mem.write_u8(0x0600_0002, 0x12);
        assert_eq!(mem.read_u16(0x0600_0002), 0x1212);
        mem.write_u8(0x0700_0000, 0x12);
        assert_eq!(mem.read_u16(0x0700_0000), 0);
    }

    #[test]
    fn test_bios_reads_outside_bios_return_last_fetched_opcode() {
        let mut bios = vec![0u8; BIOS_LEN];
        bios[0x100..0x104].copy_from_slice(&0x1234_5678u32.to_le_bytes());
        let mut mem = Memory::new(Bios::Image(bios), vec![]);
        assert_eq!(mem.fetch_u32(0x100), 0x1234_5678);
        assert_eq!(mem.read_u32(0x200), 0);
        mem.fetch_u32(0x0800_0000);
        assert_eq!(mem.read_u32(0x200), 0x1234_5678);
        assert_eq!(mem.read_u16(0x202), 0x1234);
        assert_eq!(mem.read_u8(0x201), 0x56);
    }

    #[test]
    fn test_unmapped_reads_return_the_last_prefetched_opcode() {
        let mut mem = memory(vec![]);
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

    #[test]
    fn test_prefetch_buffer_serves_sequential_fetches_through_memory() {
        let mut mem = memory(vec![0; 0x100]);
        mem.write_u16(0x0400_0204, 0x4000);
        mem.fetch_u16(0x0800_0000);
        assert_eq!(mem.take_cycles(), 5);
        mem.fetch_u16(0x0800_0002);
        assert_eq!(mem.take_cycles(), 3);
    }
}
