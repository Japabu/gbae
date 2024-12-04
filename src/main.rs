mod bitutil;
mod cartridge;
mod system;

use cartridge::CartridgeInfo;
use system::{cpu::CPU, memory::Memory};
use std::fs;

fn main() {
    let bios = fs::read("gba_bios.bin").expect("Failed to read bios");
    let cartridge_data = fs::read("rom.gba").expect("Failed to read cartridge");
    let cartridge = CartridgeInfo::parse(&cartridge_data).expect("Failed to parse cartridge info");
    println!("Title: {}", cartridge.title);

    let memory = Memory::new(bios, cartridge_data);
    let mut cpu = CPU::new(memory);
    loop {
        cpu.cycle();
    }
}
