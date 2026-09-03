use crate::bits::Bits;

use crate::system::state::{Reader, StateError, Writer};

const PREFETCH_CAPACITY: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Nonsequential,
    Sequential,
}

#[inline]
pub fn is_rom(address: u32) -> bool {
    (0x08..=0x0D).contains(&(address >> 24))
}

#[inline]
fn rom_wait_state(address: u32) -> usize {
    ((address >> 25) - 4) as usize
}

#[derive(Debug, Clone, Copy)]
struct WaitStates {
    rom_first: [u32; 3],
    rom_next: [u32; 3],
    sram: u32,
    prefetch: bool,
}

impl WaitStates {
    fn decode(wait_cnt: u16) -> WaitStates {
        let first = |bits: u16| match bits {
            0 => 5,
            1 => 4,
            2 => 3,
            _ => 9,
        };
        let next = |fast: bool, slow: u32| if fast { 2 } else { slow };
        WaitStates {
            rom_first: [first(wait_cnt.bits(2..4)), first(wait_cnt.bits(5..7)), first(wait_cnt.bits(8..10))],
            rom_next: [next(wait_cnt.bit(4), 3), next(wait_cnt.bit(7), 5), next(wait_cnt.bit(10), 9)],
            sram: first(wait_cnt.bits(0..2)),
            prefetch: wait_cnt.bit(14),
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

pub struct Bus {
    wait: WaitStates,
    prefetch: Prefetch,
    cycles: u32,
    next_fetch: Option<u32>,
}

impl Bus {
    pub fn new() -> Bus {
        Bus {
            wait: WaitStates::decode(0),
            prefetch: Prefetch::default(),
            cycles: 0,
            next_fetch: None,
        }
    }

    pub fn configure(&mut self, wait_cnt: u16) {
        self.wait = WaitStates::decode(wait_cnt);
    }

    #[inline(always)]
    pub fn take_cycles(&mut self) -> u32 {
        std::mem::take(&mut self.cycles)
    }

    #[inline]
    pub fn add_cycles(&mut self, cycles: u32) {
        self.cycles += cycles;
    }

    #[inline(always)]
    pub fn idle(&mut self, cycles: u32) {
        self.cycles += cycles;
        self.advance_prefetch(cycles);
    }

    #[inline]
    pub fn invalidate_sequence(&mut self) {
        self.next_fetch = None;
    }

    #[inline]
    pub fn interrupt(&mut self) {
        self.prefetch.active = false;
        self.next_fetch = None;
    }

    #[inline(always)]
    pub fn access_cycles(&self, address: u32, bytes: u32, access: Access) -> u32 {
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
                let wait_state = rom_wait_state(address);
                let sequential = access == Access::Sequential && address.bits(0..17) != 0;
                let first = if sequential { self.wait.rom_next[wait_state] } else { self.wait.rom_first[wait_state] };
                if word {
                    first + self.wait.rom_next[wait_state]
                } else {
                    first
                }
            }
            0x0E | 0x0F => self.wait.sram,
            _ => 1,
        }
    }

    #[inline(always)]
    fn rom_next_cycles(&self, address: u32) -> u32 {
        self.wait.rom_next[rom_wait_state(address)]
    }

    #[inline(always)]
    fn advance_prefetch(&mut self, cycles: u32) {
        if !self.prefetch.active || self.prefetch.buffered >= PREFETCH_CAPACITY {
            return;
        }
        let per_halfword = self.rom_next_cycles(self.prefetch.start);
        self.prefetch.progress += cycles;
        while self.prefetch.progress >= per_halfword && self.prefetch.buffered < PREFETCH_CAPACITY {
            self.prefetch.progress -= per_halfword;
            self.prefetch.buffered += 1;
        }
        if self.prefetch.buffered == PREFETCH_CAPACITY {
            self.prefetch.progress = 0;
        }
    }

    #[inline(always)]
    fn prefetched_halfword_cycles(&mut self, halfword: u32) -> u32 {
        if self.prefetch.active && halfword == self.prefetch.start {
            self.prefetch.start += 2;
            if self.prefetch.buffered > 0 {
                self.prefetch.buffered -= 1;
                self.advance_prefetch(1);
                1
            } else {
                let remaining = self.rom_next_cycles(halfword) - self.prefetch.progress;
                self.prefetch.progress = 0;
                remaining
            }
        } else {
            self.prefetch = Prefetch {
                active: true,
                start: halfword + 2,
                buffered: 0,
                progress: 0,
            };
            self.access_cycles(halfword, 2, Access::Nonsequential)
        }
    }

    #[inline(always)]
    fn prefetched_fetch_cycles(&mut self, address: u32, bytes: u32) -> u32 {
        let first = self.prefetched_halfword_cycles(address);
        if bytes == 4 {
            first + self.prefetched_halfword_cycles(address + 2)
        } else {
            first
        }
    }

    #[inline(always)]
    pub fn charge_fetch(&mut self, address: u32, bytes: u32) {
        let access = if self.next_fetch == Some(address) { Access::Sequential } else { Access::Nonsequential };
        let cycles = if is_rom(address) && self.wait.prefetch {
            self.prefetched_fetch_cycles(address, bytes)
        } else {
            if !is_rom(address) {
                self.prefetch.active = false;
            }
            self.access_cycles(address, bytes, access)
        };
        self.cycles += cycles;
        self.next_fetch = Some(address.wrapping_add(bytes));
    }

    #[inline(always)]
    pub fn charge_data(&mut self, address: u32, bytes: u32, access: Access) {
        let cycles = self.access_cycles(address, bytes, access);
        self.cycles += cycles;
        self.next_fetch = None;
        if is_rom(address) {
            self.prefetch.active = false;
        } else {
            self.advance_prefetch(cycles);
        }
    }

    pub fn save_state(&self, writer: &mut Writer) {
        writer.bool(self.prefetch.active);
        writer.u32(self.prefetch.start);
        writer.u32(self.prefetch.buffered);
        writer.u32(self.prefetch.progress);
        writer.u32(self.cycles);
        writer.bool(self.next_fetch.is_some());
        writer.u32(self.next_fetch.unwrap_or(0));
    }

    pub fn load_state(&mut self, reader: &mut Reader, wait_cnt: u16) -> Result<(), StateError> {
        self.wait = WaitStates::decode(wait_cnt);
        self.prefetch.active = reader.bool()?;
        self.prefetch.start = reader.u32()?;
        self.prefetch.buffered = reader.u32()?;
        self.prefetch.progress = reader.u32()?;
        self.cycles = reader.u32()?;
        let sequential = reader.bool()?;
        let address = reader.u32()?;
        self.next_fetch = sequential.then_some(address);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wait_state_decoding() {
        let default = WaitStates::decode(0);
        assert_eq!(default.rom_first, [5, 5, 5]);
        assert_eq!(default.rom_next, [3, 5, 9]);
        assert_eq!(default.sram, 5);
        assert!(!default.prefetch);
        let bios_setting = WaitStates::decode(0x4317);
        assert_eq!(bios_setting.rom_first[0], 4);
        assert_eq!(bios_setting.rom_next[0], 2);
        assert_eq!(bios_setting.sram, 9);
        assert!(bios_setting.prefetch);
    }

    #[test]
    fn test_access_cycles_per_region() {
        let bus = Bus::new();
        assert_eq!(bus.access_cycles(0x0300_0000, 4, Access::Nonsequential), 1);
        assert_eq!(bus.access_cycles(0x0200_0000, 2, Access::Sequential), 3);
        assert_eq!(bus.access_cycles(0x0200_0000, 4, Access::Sequential), 6);
        assert_eq!(bus.access_cycles(0x0600_0000, 4, Access::Sequential), 2);
        assert_eq!(bus.access_cycles(0x0800_0000, 2, Access::Nonsequential), 5);
        assert_eq!(bus.access_cycles(0x0800_0002, 2, Access::Sequential), 3);
        assert_eq!(bus.access_cycles(0x0800_0000, 4, Access::Nonsequential), 8);
        assert_eq!(bus.access_cycles(0x0802_0000, 2, Access::Sequential), 5);
        assert_eq!(bus.access_cycles(0x0C00_0004, 4, Access::Sequential), 18);
        assert_eq!(bus.access_cycles(0x0E00_0000, 1, Access::Nonsequential), 5);
    }

    #[test]
    fn test_prefetch_buffer_serves_sequential_fetches() {
        let mut bus = Bus::new();
        bus.configure(0x4000);
        bus.charge_fetch(0x0800_0000, 2);
        assert_eq!(bus.take_cycles(), 5);
        bus.charge_fetch(0x0800_0002, 2);
        assert_eq!(bus.take_cycles(), 3);
        bus.idle(7);
        bus.take_cycles();
        bus.charge_fetch(0x0800_0004, 2);
        assert_eq!(bus.take_cycles(), 1);
        bus.charge_fetch(0x0800_0006, 2);
        assert_eq!(bus.take_cycles(), 1);
        bus.charge_data(0x0800_0040, 4, Access::Nonsequential);
        bus.take_cycles();
        bus.charge_fetch(0x0800_0008, 2);
        assert_eq!(bus.take_cycles(), 5);
    }
}
