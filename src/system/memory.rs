/*
GBA Memory Map
General Internal Memory
  0000_0000-0000_3FFF   BIOS - System ROM         (16 KBytes)
  0000_4000-01FF_FFFF   Not used
  0200_0000-0203_FFFF   WRAM - On-board Work RAM  (256 KBytes) 2 Wait
  0204_0000-02FF_FFFF   Not used
  0300_0000-0300_7FFF   WRAM - On-chip Work RAM   (32 KBytes)
  0300_8000-03FF_FFFF   Not used
  0400_0000-0400_03FE   I/O Registers
  0400_0400-04FF_FFFF   Not used
Internal Display Memory
  0500_0000-0500_03FF   BG/OBJ Palette RAM        (1 Kbyte)
  0500_0400-05FF_FFFF   Not used
  0600_0000-0601_7FFF   VRAM - Video RAM          (96 KBytes)
  0601_8000-06FF_FFFF   Not used
  0700_0000-0700_03FF   OAM - OBJ Attributes      (1 Kbyte)
  0700_0400-07FF_FFFF   Not used
External Memory (Game Pak)
  0800_0000-09FF_FFFF   Game Pak ROM/FlashROM (max 32MB) - Wait State 0
  0A00_0000-0BFF_FFFF   Game Pak ROM/FlashROM (max 32MB) - Wait State 1
  0C00_0000-0DFF_FFFF   Game Pak ROM/FlashROM (max 32MB) - Wait State 2
  0E00_0000-0E00_FFFF   Game Pak SRAM    (max 64 KBytes) - 8bit Bus width
  0E01_0000-0FFF_FFFF   Not used
Unused Memory Area
  1000_0000-FFFF_FFFF   Not used (upper 4bits of address bus unused)
*/

macro_rules! gen_memory {
    ($($start:literal..=$end:literal => ($region:ident, $writable:expr)),* $(,)?) => {
        pub struct Memory {
            $(
                $region: Vec<u8>,
            )*
        }

        impl Memory {
            pub fn read_u8(&self, address: usize) -> u8 {
                match address {
                    $($start..=$end => self.$region[address - $start],)*
                    _ => panic!("Read from unmapped address: {:#08X}", address),
                }
            }

            pub fn write_u8(&mut self, address: usize, value: u8) {
                match address {
                    $(
                        $start..=$end => {
                            if $writable { self.$region[address - $start] = value }
                            else { panic!("Write to read-only address: {:#08X}", address) }
                        }
                    ,)*
                    _ => panic!("Write to unmapped address: {:#08X}", address),
                }
            }
        }
    };
}

gen_memory! {
    0x0000_0000..=0x0000_3FFF => (bios, false),
    0x0200_0000..=0x0203_FFFF => (wram1, true),
    0x0300_0000..=0x0300_7FFF => (wram2, true),
    0x0800_0000..=0x09FF_FFFF => (game_pak, false),
}

impl Memory {
    pub fn new(bios: Vec<u8>, game_pak: Vec<u8>) -> Self {
        Self {
            bios,
            game_pak,
            wram1: vec![0; 0x4_0000],
            wram2: vec![0; 0x800],
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

    pub fn write_u16(&mut self, address: usize, value: u16) {
        self.write_u8(address, value as u8);
        self.write_u8(address + 1, (value >> 8) as u8);
    }

    pub fn write_u32(&mut self, address: usize, value: u32) {
        self.write_u16(address, value as u16);
        self.write_u16(address + 2, (value >> 16) as u16);
    }

    fn write(&mut self, address: usize, data: &[u8]) {
        for (i, byte) in data.iter().enumerate() {
            self.write_u8(address + i, *byte);
        }
    }
}
