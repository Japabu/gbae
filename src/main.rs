#![feature(type_alias_impl_trait)]
#![feature(bigint_helper_methods)]

mod bitutil;
mod cartridge;
mod debugger;
mod mcp;
mod system;

use cartridge::CartridgeInfo;
use debugger::Debugger;
use std::fs;
use system::{cpu::CPU, display::{Display, DisplayEvent}, memory::Memory, ppu::PPU};
use winit::event_loop::ControlFlow;

fn main() {
    let bios = fs::read("gba_bios.bin").expect("Failed to read BIOS");
    let cartridge_data = fs::read("rom.gba").expect("Failed to read ROM");
    let cartridge = CartridgeInfo::parse(&cartridge_data).expect("Failed to parse cartridge");
    eprintln!("Cartridge: {}", cartridge.title);

    // Create command channel for MCP communication
    let (command_tx, command_rx) = mcp::create_channel();

    // Get MCP port from environment or use default
    let mcp_port = std::env::var("GBA_MCP_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    // Create display and event loop
    let (ppu, framebuffer) = PPU::new();
    let (mut display, event_loop) = Display::new(framebuffer.clone());
    let event_loop_proxy = event_loop.create_proxy();

    // Start CLI thread for console input
    let cli_command_tx = command_tx.clone();
    std::thread::spawn(move || {
        use std::io::{self, BufRead};
        let stdin = io::stdin();
        eprintln!("CLI ready. Commands: c (continue), h (halt), s [n] (step), r (registers), help");

        for line in stdin.lock().lines() {
            if let Ok(line) = line {
                let parts: Vec<&str> = line.trim().split_whitespace().collect();
                if parts.is_empty() {
                    continue;
                }

                match parts[0] {
                    "c" | "continue" => {
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        let _ = cli_command_tx.send(mcp::CommandRequest {
                            command: mcp::DebugCommand::Continue,
                            response_tx: tx,
                        });
                        // Don't wait for response for continue
                        eprintln!("Continuing execution...");
                    }
                    "h" | "halt" => {
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        let _ = cli_command_tx.send(mcp::CommandRequest {
                            command: mcp::DebugCommand::Halt,
                            response_tx: tx,
                        });
                        eprintln!("Halting execution...");
                    }
                    "s" | "step" => {
                        let count = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        let _ = cli_command_tx.send(mcp::CommandRequest {
                            command: mcp::DebugCommand::Step { count },
                            response_tx: tx,
                        });
                        eprintln!("Stepping {} instruction(s)...", count);
                    }
                    "r" | "regs" | "registers" => {
                        let (tx, mut rx) = tokio::sync::oneshot::channel();
                        let _ = cli_command_tx.send(mcp::CommandRequest {
                            command: mcp::DebugCommand::GetCpuState,
                            response_tx: tx,
                        });
                        // Try to get response (non-blocking)
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        match rx.try_recv() {
                            Ok(mcp::DebugResponse::CpuState { registers, cpsr, pc }) => {
                                eprintln!("PC: {:#010X}  CPSR: {:#010X}", pc, cpsr);
                                for i in 0..16 {
                                    eprintln!("r{:<2}: {:#010X}", i, registers[i]);
                                }
                            }
                            _ => eprintln!("Failed to get CPU state"),
                        }
                    }
                    "help" => {
                        eprintln!("Available commands:");
                        eprintln!("  c, continue       - Continue execution");
                        eprintln!("  h, halt           - Halt execution");
                        eprintln!("  s [n], step [n]   - Step n instructions (default 1)");
                        eprintln!("  r, regs           - Show CPU registers");
                        eprintln!("  help              - Show this help");
                    }
                    _ => {
                        eprintln!("Unknown command: {}. Type 'help' for available commands.", parts[0]);
                    }
                }
            }
        }
    });

    // Start MCP server in a separate thread
    let mcp_command_tx = command_tx.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            if let Err(e) = mcp::start_mcp_server(mcp_port, mcp_command_tx).await {
                eprintln!("MCP server error: {}", e);
            }
        });
    });

    // Run emulator in background thread
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                run_emulator(bios, cartridge_data, command_rx, ppu, event_loop_proxy, framebuffer).await;
            });
        }));

        if result.is_err() {
            eprintln!("\n!!! EMULATOR CRASHED - Exiting !!!\n");
            std::process::exit(1);
        }
    });

    // Run display on main thread
    event_loop.set_control_flow(ControlFlow::Poll);
    event_loop.run_app(&mut display).unwrap();
}

async fn run_emulator(
    bios: Vec<u8>,
    cartridge_data: Vec<u8>,
    command_rx: mcp::CommandReceiver,
    mut ppu: PPU,
    event_loop_proxy: winit::event_loop::EventLoopProxy<DisplayEvent>,
    framebuffer: std::sync::Arc<std::sync::RwLock<system::ppu::Framebuffer>>,
) {
    let mut mem = Memory::new(bios, cartridge_data);
    let mut cpu = CPU::new();
    let mut debugger = Debugger::new()
        .with_mcp(command_rx)
        .with_framebuffer(framebuffer);

    // Start HALTED - user must type 'c' or use MCP to start
    debugger.running = false;
    eprintln!("Emulator started in HALTED state. Type 'c' to continue or 'help' for commands.");

    let mut scanline_counter = 0;
    let mut last_progress_log = std::time::Instant::now();
    let mut instruction_count: u64 = 0;

    loop {
        // Handle MCP commands
        debugger.handle_mcp_commands(&mut cpu, &mut mem).await;

        if debugger.running {
            const CYCLES_PER_SCANLINE: u64 = 1232;
            const INSTRUCTIONS_PER_BATCH: u32 = 100000; // Execute many instructions per batch
            const INTERRUPT_CHECK_INTERVAL: u32 = 100; // Check interrupts every 100 instructions

            // Execute a batch of instructions before checking MCP commands again
            for i in 0..INSTRUCTIONS_PER_BATCH {
                instruction_count += 1;

                // Only check timing/interrupts periodically to reduce overhead
                if i % INTERRUPT_CHECK_INTERVAL == 0 {
                    // Update v_count BEFORE cpu.cycle() so it sees the current value
                    let target_scanline = cpu.get_cycles() / CYCLES_PER_SCANLINE;
                    let current_v_count = (target_scanline % 228) as u16;
                    let prev_v_count = mem.get_io_registers().v_count;
                    mem.get_io_registers_mut().v_count = current_v_count;

                    // Generate VBlank interrupt when transitioning from line 159 to 160
                    if prev_v_count < 160 && current_v_count >= 160 {
                        // Set VBlank bit (bit 0) in IRF register
                        let io = mem.get_io_registers_mut();
                        io.irf |= 0x0001;  // VBlank interrupt

                        // Also update BIOS interrupt flags at 0x03007FF8
                        let bios_if = mem.read_u16(0x03007FF8);
                        mem.write_u16(0x03007FF8, bios_if | 0x0001);
                    }

                    // Handle interrupts
                    cpu.handle_interrupts(&mut mem);
                }

                cpu.cycle(&mut mem);

                // Check for breakpoints
                let pc = cpu.get_r(15);
                if debugger.check_breakpoint(pc) {
                    break; // Exit batch loop to handle debugger commands
                }

                // Stop batch if no longer running (halted via breakpoint or command)
                if !debugger.running {
                    break;
                }
            }

            // Draw scanlines AFTER the instruction batch to reduce per-instruction overhead
            let target_scanline = cpu.get_cycles() / CYCLES_PER_SCANLINE;
            while target_scanline > scanline_counter {
                scanline_counter += 1;
                let v_count = (scanline_counter % 228) as u16;

                // Update v_count register BEFORE drawing so PPU sees correct value
                mem.get_io_registers_mut().v_count = v_count;

                if v_count < 160 {
                    ppu.draw_scanline(&mut mem);
                }

                // Only request redraw every 4 frames to reduce overhead during BIOS
                if v_count == 159 && (scanline_counter / 228) % 4 == 0 {
                    let _ = event_loop_proxy.send_event(DisplayEvent::RedrawRequested);
                }
            }

            // Log progress every 5 seconds
            if last_progress_log.elapsed().as_secs() >= 5 {
                let pc = cpu.get_r(15);
                eprintln!("[Progress] PC: 0x{:08X}, Instructions: {}, Rate: {:.1}M instr/sec",
                    pc, instruction_count, instruction_count as f64 / last_progress_log.elapsed().as_secs_f64() / 1_000_000.0);

                // Check if we've reached ROM
                if pc >= 0x08000000 && pc < 0x0A000000 {
                    eprintln!("*** BIOS INITIALIZATION COMPLETE - ROM REACHED! ***");
                }

                last_progress_log = std::time::Instant::now();
                instruction_count = 0;
            }
        } else {
            // Sleep briefly when halted to avoid spinning
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    }
}
