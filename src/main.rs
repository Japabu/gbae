mod bitutil;
mod cpu;
mod cartridge;

use std::fs;

use cartridge::CartridgeInfo;
use cpu::CPU;



fn main() {
    let mut data = fs::read("rom.gba").expect("Failed to read cartridge");
    let cartridge = CartridgeInfo::parse(&data).expect("Failed to parse cartridge info");
    println!("Title: {}", cartridge.title);

    let mut cpu = CPU::new(&mut data);
    loop {
        cpu.cycle();
    }
}
