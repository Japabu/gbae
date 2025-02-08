#![feature(type_alias_impl_trait)]
#![feature(bigint_helper_methods)]

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
use system::{
    cpu::CPU,
    display::{Display, DisplayEvent},
    memory::Memory,
    ppu::PPU,
};
use winit::event_loop::ControlFlow;

fn main() {
    let bios = fs::read("gba_bios.bin").expect("Failed to read bios");
    let cartridge_data = fs::read("rom.gba").expect("Failed to read cartridge");
    let cartridge = CartridgeInfo::parse(&cartridge_data).expect("Failed to parse cartridge info");
    println!("Title: {}", cartridge.title);

    let (mut ppu, framebuffer) = PPU::new();
    let (mut display, event_loop) = Display::new(framebuffer);
    let event_loop_proxy = event_loop.create_proxy();

    // Spawn emulator thread
    std::thread::spawn(move || {
        let mut mem = Memory::new(bios, cartridge_data);
        let mut cpu = CPU::new();
        let mut debugger = Debugger::new();

        println!("GBA Debugger. Type 'h' for help.");

        loop {
            // Print current instruction before executing it
            println!();
            cpu.print_registers();
            cpu.print_status();
            println!("{:08X}: {:08X}", 0x03007E9C, mem.read_u32(0x03007E9C));
            cpu.print_next_instruction(&mem);

            if !debugger.running || debugger.should_break(&cpu) {
                debugger.running = false;
                print!("> ");
                stdout().flush().unwrap();

                let mut input = String::new();
                stdin().read_line(&mut input).unwrap();
                debugger.handle_command(&input, &mut cpu, &mut mem);
            }

            if debugger.running {
                cpu.cycle(&mut mem);
                const CPU_CYCLES_PER_FRAME: u64 = 2273;
                while cpu.get_cycles() / CPU_CYCLES_PER_FRAME > ppu.get_frame_counter() {
                    ppu.draw_frame(&mut mem);
                    event_loop_proxy.send_event(DisplayEvent::RedrawRequested).unwrap();
                }
            }
        }
    });

    // Run display on main thread
    event_loop.set_control_flow(ControlFlow::Wait);

    #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
    event_loop.run_app(&mut display).unwrap();

    #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
    winit::platform::web::EventLoopExtWebSys::spawn_app(event_loop, display);
}
