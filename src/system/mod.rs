use cpu::CPU;
use memory::Memory;

mod instructions;
mod memory;
pub mod cpu;

pub struct GbaSystem<'a> {
    mem: Memory,
    cpu: CPU<'a>,
}

impl<'a> GbaSystem<'a> {
    pub fn new(bios: Vec<u8>, cartridge: Vec<u8>) -> Self {
        let mut mem = Memory::new(bios, cartridge);
        Self {
            mem,
            cpu: CPU::new(&mut mem),
        }
    }
}