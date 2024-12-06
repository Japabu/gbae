mod bitutil;
mod cartridge;
mod debugger;
mod system;

use cartridge::CartridgeInfo;
use debugger::Debugger;
use std::{
    fs,
    io::{stdin, stdout, Write},
};
use system::{cpu::CPU, instructions::format_instruction, memory::Memory};

fn main() {
    let bios = fs::read("gba_bios.bin").expect("Failed to read bios");
    let cartridge_data = fs::read("rom.gba").expect("Failed to read cartridge");
    let cartridge = CartridgeInfo::parse(&cartridge_data).expect("Failed to parse cartridge info");
    println!("Title: {}", cartridge.title);

    let memory = Memory::new(bios, cartridge_data);
    let mut cpu = CPU::new(memory);
    let mut debugger = Debugger::new();

    println!("GBA Debugger. Type 'h' for help.");

    loop {
        // Print current instruction before executing it
        let instruction = cpu.peek_next_instruction();
        println!(
            "Next instruction at 0x{:08X}: {}",
            cpu.get_r(15),
            format_instruction(instruction)
        );

        if !debugger.running || debugger.should_break(&cpu) {
            debugger.running = false;
            print!("> ");
            stdout().flush().unwrap();

            let mut input = String::new();
            stdin().read_line(&mut input).unwrap();
            debugger.handle_command(&mut cpu, &input);
        }

        if debugger.running {
            cpu.cycle();
        }
    }
}
