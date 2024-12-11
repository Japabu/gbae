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

macro_rules! gen_memory {
    ($($start:literal..=$end:literal => ($region:ident, $len:expr, $writable:expr)),* $(,)?) => {
        pub struct Memory {
            $(
                $region: Vec<u8>,
            )*
        }

        impl Memory {
            pub fn read_u8(&self, address: u32) -> u8 {
                match address {
                    $(
                        $start..=$end => {
                            let index = (address - $start) % $len;
                            self.$region[index as usize]
                        }
                    )*
                    _ => panic!("Read from unmapped address: {:#08X}", address),
                }
            }

            pub fn write_u8(&mut self, address: u32, value: u8) {
                match address {
                    $(
                        $start..=$end => {
                            let index = (address - $start) % $len;
                            if $writable { self.$region[index as usize] = value }
                            else { panic!("Write to read-only address: {:#08X}", address) }
                        }
                    ,)*
                    _ => panic!("Write to unmapped address: {:#08X}", address),
                }
            }
        }
    };
}

const BIOS_LEN: u32 = 0x4_000;
const WRAM1_LEN: u32 = 0x40_000;
const WRAM2_LEN: u32 = 0x800;
const IO_REGISTERS_LEN: u32 = 0x3FF;
const GAME_PAK_ROM_LEN: u32 = 0x2_000_000;

gen_memory! {
    0x00_000_000..=0x00_003_FFF => (bios, BIOS_LEN, false),
    0x02_000_000..=0x02_FFF_FFF => (wram1, WRAM1_LEN, true),
    0x03_000_000..=0x03_FFF_FFF => (wram2, WRAM2_LEN, true),
    0x04_000_000..=0x04_000_3FE => (io_registers, IO_REGISTERS_LEN, true),
    0x08_000_000..=0x09_FFF_FFF => (game_pak, GAME_PAK_ROM_LEN, false),
}

impl Memory {
    pub fn new(bios: Vec<u8>, game_pak: Vec<u8>) -> Self {
        Self {
            bios,
            wram1: vec![0; WRAM1_LEN as usize],
            wram2: vec![0; WRAM2_LEN as usize],
            io_registers: vec![0; IO_REGISTERS_LEN as usize],
            game_pak,
        }
    }

    pub fn read_u16(&self, address: u32) -> u16 {
        let low = self.read_u8(address) as u16;
        let high = self.read_u8(address + 1) as u16;
        (high << 8) | low
    }

    pub fn read_u32(&self, address: u32) -> u32 {
        let low = self.read_u16(address) as u32;
        let high = self.read_u16(address + 2) as u32;
        (high << 16) | low
    }

    pub fn write_u16(&mut self, address: u32, value: u16) {
        self.write_u8(address, value as u8);
        self.write_u8(address + 1, (value >> 8) as u8);
    }

    pub fn write_u32(&mut self, address: u32, value: u32) {
        self.write_u16(address, value as u16);
        self.write_u16(address + 2, (value >> 16) as u16);
    }
}
