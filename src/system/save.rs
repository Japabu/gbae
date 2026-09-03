use std::cell::Cell;

use super::state::{Reader, StateError, Writer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveType {
    None,
    Sram,
    Flash64K,
    Flash128K,
    Eeprom,
}

impl SaveType {
    pub fn detect(rom: &[u8]) -> SaveType {
        const MARKERS: [(&[u8], SaveType); 5] = [
            (b"EEPROM_V", SaveType::Eeprom),
            (b"SRAM_V", SaveType::Sram),
            (b"FLASH1M_V", SaveType::Flash128K),
            (b"FLASH512_V", SaveType::Flash64K),
            (b"FLASH_V", SaveType::Flash64K),
        ];
        for offset in (0..rom.len()).step_by(4) {
            for (marker, save_type) in MARKERS {
                if rom[offset..].starts_with(marker) {
                    return save_type;
                }
            }
        }
        SaveType::None
    }
}

pub struct Backup {
    medium: Medium,
    dirty: bool,
}

enum Medium {
    None,
    Sram(Vec<u8>),
    Flash(Flash),
    Eeprom(Eeprom),
}

const SRAM_LEN: usize = 0x8000;

impl Backup {
    pub fn new(save_type: SaveType) -> Backup {
        let medium = match save_type {
            SaveType::None => Medium::None,
            SaveType::Sram => Medium::Sram(vec![0xFF; SRAM_LEN]),
            SaveType::Flash64K => Medium::Flash(Flash::new(0x1_0000, [0x32, 0x1B])),
            SaveType::Flash128K => Medium::Flash(Flash::new(0x2_0000, [0xC2, 0x09])),
            SaveType::Eeprom => Medium::Eeprom(Eeprom::new()),
        };
        Backup { medium, dirty: false }
    }

    pub fn save_type(&self) -> SaveType {
        match &self.medium {
            Medium::None => SaveType::None,
            Medium::Sram(_) => SaveType::Sram,
            Medium::Flash(flash) if flash.data.len() == 0x1_0000 => SaveType::Flash64K,
            Medium::Flash(_) => SaveType::Flash128K,
            Medium::Eeprom(_) => SaveType::Eeprom,
        }
    }

    pub fn read(&self, address: u32) -> u8 {
        match &self.medium {
            Medium::None | Medium::Eeprom(_) => 0xFF,
            Medium::Sram(data) => data[address as usize & (SRAM_LEN - 1)],
            Medium::Flash(flash) => flash.read(address),
        }
    }

    pub fn write(&mut self, address: u32, value: u8) {
        let changed = match &mut self.medium {
            Medium::None | Medium::Eeprom(_) => false,
            Medium::Sram(data) => {
                let slot = &mut data[address as usize & (SRAM_LEN - 1)];
                let changed = *slot != value;
                *slot = value;
                changed
            }
            Medium::Flash(flash) => flash.write(address, value),
        };
        self.dirty |= changed;
    }

    pub fn is_eeprom(&self) -> bool {
        matches!(self.medium, Medium::Eeprom(_))
    }

    pub fn eeprom_begin_transfer(&mut self, length: u32) {
        if let Medium::Eeprom(eeprom) = &mut self.medium {
            eeprom.begin_transfer(length);
        }
    }

    pub fn eeprom_read(&self) -> u16 {
        match &self.medium {
            Medium::Eeprom(eeprom) => eeprom.read(),
            _ => 1,
        }
    }

    pub fn eeprom_write(&mut self, value: u16) {
        if let Medium::Eeprom(eeprom) = &mut self.medium {
            self.dirty |= eeprom.write(value & 1 != 0);
        }
    }

    pub fn data(&self) -> &[u8] {
        match &self.medium {
            Medium::None => &[],
            Medium::Sram(data) => data,
            Medium::Flash(flash) => &flash.data,
            Medium::Eeprom(eeprom) => &eeprom.data,
        }
    }

    pub fn load(&mut self, bytes: &[u8]) {
        let data = match &mut self.medium {
            Medium::None => return,
            Medium::Sram(data) => data,
            Medium::Flash(flash) => &mut flash.data,
            Medium::Eeprom(eeprom) => &mut eeprom.data,
        };
        let length = bytes.len().min(data.len());
        data[..length].copy_from_slice(&bytes[..length]);
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.dirty, false)
    }

    pub fn save_state(&self, writer: &mut Writer) {
        writer.u8(self.save_type() as u8);
        writer.sized_bytes(self.data());
        match &self.medium {
            Medium::Flash(flash) => flash.save_state(writer),
            Medium::Eeprom(eeprom) => eeprom.save_state(writer),
            Medium::None | Medium::Sram(_) => {}
        }
    }

    pub fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        if reader.u8()? != self.save_type() as u8 {
            return Err(StateError::Corrupt);
        }
        let data = reader.sized_bytes()?;
        if data.len() != self.data().len() {
            return Err(StateError::Corrupt);
        }
        self.load(data);
        match &mut self.medium {
            Medium::Flash(flash) => flash.load_state(reader)?,
            Medium::Eeprom(eeprom) => eeprom.load_state(reader)?,
            Medium::None | Medium::Sram(_) => {}
        }
        self.dirty = true;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl FlashCommand {
    fn from_state(value: u8) -> Result<FlashCommand, StateError> {
        [
            FlashCommand::None,
            FlashCommand::Prefix1,
            FlashCommand::Prefix2,
            FlashCommand::ErasePrefix1,
            FlashCommand::ErasePrefix2,
            FlashCommand::ErasePrefix3,
            FlashCommand::Program,
            FlashCommand::BankSwitch,
        ]
        .get(value as usize)
        .copied()
        .ok_or(StateError::Corrupt)
    }
}

pub struct Flash {
    data: Vec<u8>,
    id: [u8; 2],
    bank: usize,
    id_mode: bool,
    command: FlashCommand,
}

impl Flash {
    fn new(size: usize, id: [u8; 2]) -> Flash {
        Flash {
            data: vec![0xFF; size],
            id,
            bank: 0,
            id_mode: false,
            command: FlashCommand::None,
        }
    }

    fn address(&self, offset: u32) -> usize {
        self.bank * 0x10000 + (offset & 0xFFFF) as usize
    }

    fn save_state(&self, writer: &mut Writer) {
        writer.u8(self.bank as u8);
        writer.bool(self.id_mode);
        writer.u8(self.command as u8);
    }

    fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.bank = reader.u8()? as usize % (self.data.len() / 0x10000);
        self.id_mode = reader.bool()?;
        self.command = FlashCommand::from_state(reader.u8()?)?;
        Ok(())
    }

    fn read(&self, offset: u32) -> u8 {
        if self.id_mode {
            match offset & 0xFFFF {
                0 => self.id[0],
                1 => self.id[1],
                _ => 0,
            }
        } else {
            self.data[self.address(offset)]
        }
    }

    fn write(&mut self, offset: u32, value: u8) -> bool {
        let offset = offset & 0xFFFF;
        let mut changed = false;
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
                    changed = true;
                } else if value == 0x30 {
                    let start = self.bank * 0x10000 + (offset & 0xF000) as usize;
                    self.data[start..start + 0x1000].fill(0xFF);
                    changed = true;
                }
                self.command = FlashCommand::None;
            }
            FlashCommand::Program => {
                let address = self.address(offset);
                changed = self.data[address] & value != self.data[address];
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
        changed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EepromState {
    Idle,
    Receiving,
    Reading,
}

pub struct Eeprom {
    data: Vec<u8>,
    address_bits: u32,
    state: EepromState,
    received_bits: u32,
    request: u128,
    read_data: u64,
    read_position: Cell<u32>,
}

const EEPROM_LEN: usize = 0x2000;
const READ_PREAMBLE_BITS: u32 = 4;
const DATA_BITS: u32 = 64;

impl Eeprom {
    fn new() -> Eeprom {
        Eeprom {
            data: vec![0xFF; EEPROM_LEN],
            address_bits: 14,
            state: EepromState::Idle,
            received_bits: 0,
            request: 0,
            read_data: 0,
            read_position: Cell::new(0),
        }
    }

    fn save_state(&self, writer: &mut Writer) {
        writer.u32(self.address_bits);
        writer.u8(self.state as u8);
        writer.u32(self.received_bits);
        writer.u128(self.request);
        writer.u64(self.read_data);
        writer.u32(self.read_position.get());
    }

    fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.address_bits = reader.u32()?;
        self.state = match reader.u8()? {
            0 => EepromState::Idle,
            1 => EepromState::Receiving,
            2 => EepromState::Reading,
            _ => return Err(StateError::Corrupt),
        };
        self.received_bits = reader.u32()?;
        self.request = reader.u128()?;
        self.read_data = reader.u64()?;
        self.read_position.set(reader.u32()?);
        Ok(())
    }

    fn begin_transfer(&mut self, length: u32) {
        match length {
            9 | 73 => self.address_bits = 6,
            17 | 81 => self.address_bits = 14,
            _ => {}
        }
        if length != READ_PREAMBLE_BITS + DATA_BITS {
            self.state = EepromState::Idle;
        }
    }

    fn write(&mut self, bit: bool) -> bool {
        let mut changed = false;
        if self.state != EepromState::Receiving {
            self.state = EepromState::Receiving;
            self.received_bits = 0;
            self.request = 0;
        }
        self.request = self.request << 1 | bit as u128;
        self.received_bits += 1;

        if self.received_bits >= 2 {
            let is_read = self.request >> (self.received_bits - 2) & 0b11 == 0b11;
            let request_length = if is_read { 2 + self.address_bits + 1 } else { 2 + self.address_bits + DATA_BITS + 1 };
            if self.received_bits == request_length {
                let address_shift = request_length - 2 - self.address_bits;
                let address = ((self.request >> address_shift) as u32 & ((1 << self.address_bits) - 1)) as usize & 0x3FF;
                let block = address * 8..address * 8 + 8;
                if is_read {
                    self.read_data = u64::from_be_bytes(self.data[block].try_into().unwrap());
                    self.read_position.set(0);
                    self.state = EepromState::Reading;
                } else {
                    let value = (self.request >> 1) as u64;
                    self.data[block].copy_from_slice(&value.to_be_bytes());
                    changed = true;
                    self.state = EepromState::Idle;
                }
            }
        }
        changed
    }

    fn read(&self) -> u16 {
        let position = self.read_position.get();
        if self.state == EepromState::Reading && position < READ_PREAMBLE_BITS + DATA_BITS {
            self.read_position.set(position + 1);
            if position < READ_PREAMBLE_BITS {
                0
            } else {
                (self.read_data >> (DATA_BITS - 1 - (position - READ_PREAMBLE_BITS)) & 1) as u16
            }
        } else {
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_type_detection() {
        let mut rom = vec![0u8; 0x100];
        assert_eq!(SaveType::detect(&rom), SaveType::None);
        rom[0xC0..0xC9].copy_from_slice(b"FLASH1M_V");
        assert_eq!(SaveType::detect(&rom), SaveType::Flash128K);
        rom[0xC0..0xCA].copy_from_slice(b"FLASH512_V");
        assert_eq!(SaveType::detect(&rom), SaveType::Flash64K);
        rom[0xC0..0xC8].copy_from_slice(b"EEPROM_V");
        assert_eq!(SaveType::detect(&rom), SaveType::Eeprom);
        let mut misaligned = vec![0u8; 0x100];
        misaligned[0xC1..0xC7].copy_from_slice(b"SRAM_V");
        assert_eq!(SaveType::detect(&misaligned), SaveType::None);
    }

    #[test]
    fn test_eeprom_write_and_read_back() {
        let mut eeprom = Eeprom::new();
        let address = 0x123u32;
        let value = 0x0123_4567_89AB_CDEFu64;
        eeprom.begin_transfer(81);
        for bit in [true, false] {
            eeprom.write(bit);
        }
        for i in (0..14).rev() {
            eeprom.write(address >> i & 1 != 0);
        }
        for i in (0..64).rev() {
            eeprom.write(value >> i & 1 != 0);
        }
        eeprom.write(false);
        assert_eq!(&eeprom.data[address as usize * 8..address as usize * 8 + 8], &value.to_be_bytes());

        eeprom.begin_transfer(17);
        for bit in [true, true] {
            eeprom.write(bit);
        }
        for i in (0..14).rev() {
            eeprom.write(address >> i & 1 != 0);
        }
        eeprom.write(false);
        eeprom.begin_transfer(68);
        let mut read = 0u64;
        for _ in 0..4 {
            assert_eq!(eeprom.read(), 0);
        }
        for _ in 0..64 {
            read = read << 1 | eeprom.read() as u64;
        }
        assert_eq!(read, value);
        assert_eq!(eeprom.read(), 1);
    }
}
