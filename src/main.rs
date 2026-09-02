mod debugger;
mod display;
mod mcp;

use debugger::Debugger;
use display::{Display, DisplayEvent};
use gbae::cartridge::CartridgeInfo;
use gbae::system::cpu::CPU_FREQUENCY;
use gbae::system::gba::{Gba, CYCLES_PER_SCANLINE, SCANLINES_PER_FRAME};
use gbae::system::ppu::{Framebuffer, FRAMEBUFFER_HEIGHT, FRAMEBUFFER_WIDTH};
use std::fs;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
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
    let framebuffer: Arc<RwLock<Framebuffer>> = Arc::new(RwLock::new([[[0; 3]; FRAMEBUFFER_WIDTH]; FRAMEBUFFER_HEIGHT]));
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
                run_emulator(bios, cartridge_data, command_rx, event_loop_proxy, framebuffer).await;
            });
        }));

        if result.is_err() {
            eprintln!("\n!!! EMULATOR CRASHED - Exiting !!!\n");
            std::process::exit(1);
        }
    });

    // Run display on main thread
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut display).unwrap();
}

async fn run_emulator(
    bios: Vec<u8>,
    cartridge_data: Vec<u8>,
    command_rx: mcp::CommandReceiver,
    event_loop_proxy: winit::event_loop::EventLoopProxy<DisplayEvent>,
    framebuffer: Arc<RwLock<Framebuffer>>,
) {
    let mut gba = Gba::new(bios, cartridge_data);
    let mut debugger = Debugger::new().with_mcp(command_rx);
    let frame_duration = Duration::from_secs_f64(CYCLES_PER_SCANLINE as f64 * SCANLINES_PER_FRAME as f64 / CPU_FREQUENCY as f64);
    let turbo = std::env::var_os("GBA_TURBO").is_some();

    // Start HALTED - user must type 'c' or use MCP to start
    debugger.running = false;
    eprintln!("Emulator started in HALTED state. Type 'c' to continue or 'help' for commands.");

    let mut last_progress_log = Instant::now();
    let mut step_count: u64 = 0;
    let mut frames_since_log: u64 = 0;
    let mut next_frame_deadline = Instant::now();

    loop {
        // Handle MCP commands
        debugger.handle_mcp_commands(&mut gba).await;

        if debugger.running {
            let frame = gba.frame_count();
            let has_breakpoints = debugger.has_breakpoints();

            // Run one frame before checking MCP commands again
            while gba.frame_count() == frame && debugger.running {
                step_count += 1;
                gba.step();

                if has_breakpoints && debugger.check_breakpoint(gba.cpu.pc()) {
                    break; // Exit to handle debugger commands
                }
            }

            if gba.frame_count() != frame {
                frames_since_log += 1;
                if let Ok(mut fb) = framebuffer.write() {
                    *fb = *gba.framebuffer();
                }
                let _ = event_loop_proxy.send_event(DisplayEvent::RedrawRequested);

                // Pace to real time unless GBA_TURBO is set
                if !turbo {
                    let now = Instant::now();
                    if next_frame_deadline + frame_duration * 4 < now {
                        next_frame_deadline = now;
                    }
                    next_frame_deadline += frame_duration;
                    if next_frame_deadline > now {
                        tokio::time::sleep_until(next_frame_deadline.into()).await;
                    }
                }
            }

            // Log progress every 5 seconds
            if last_progress_log.elapsed().as_secs() >= 5 {
                let elapsed = last_progress_log.elapsed().as_secs_f64();
                let pc = gba.cpu.pc();
                eprintln!(
                    "[Progress] PC: 0x{:08X}, Steps: {}, Rate: {:.1}M steps/sec, Speed: {:.2}x realtime",
                    pc,
                    step_count,
                    step_count as f64 / elapsed / 1_000_000.0,
                    frames_since_log as f64 * frame_duration.as_secs_f64() / elapsed
                );

                // Check if we've reached ROM
                if pc >= 0x08000000 && pc < 0x0A000000 {
                    eprintln!("*** BIOS INITIALIZATION COMPLETE - ROM REACHED! ***");
                }

                last_progress_log = Instant::now();
                step_count = 0;
                frames_since_log = 0;
            }
        } else {
            // Sleep briefly when halted to avoid spinning
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}
