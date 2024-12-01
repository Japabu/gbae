use core::panic;

pub struct Memory {
    bios: Vec<u8>,
    cartridge: Vec<u8>,
}

impl Memory {
    pub fn new(bios: Vec<u8>, cartridge: Vec<u8>) -> Self {
        Self {
            bios,
            cartridge,
        }
    }

    pub fn read_u8(&self, address: usize) -> u8 {
        match address {
            0x00000000..=0x00003FFF => self.bios[address - 0x00000000],
            0x08000000..=0x09FFFFFF => self.cartridge[address - 0x08000000],
            0x0A000000..=0x0BFFFFFF => self.cartridge[address - 0x0A000000],
            0x0C000000..=0x0DFFFFFF => self.cartridge[address - 0x0C000000],
            _ => panic!("Invalid address: {:#x}", address),
        }
    }

    pub fn read_u16(&self, address: usize) -> u16 {
        let low = self.read_u8(address) as u16;
        let high = self.read_u8(address + 1) as u16;
        (high << 8) | low
    }

    pub fn read_u32(&self, address: usize) -> u32 {
        let low = self.read_u16(address) as u32;
        let high = self.read_u16(address + 2) as u32;
        (high << 16) | low
    }
}