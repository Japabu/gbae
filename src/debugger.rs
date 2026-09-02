use crate::mcp::{CommandReceiver, DebugCommand, DebugResponse};
use gbae::system::gba::Gba;
use gbae::system::ppu::Framebuffer;
use std::collections::HashSet;

pub struct Debugger {
    pub running: bool,
    command_rx: Option<CommandReceiver>,
    breakpoints: HashSet<u32>,
}

impl Debugger {
    pub fn new() -> Self {
        Self {
            running: false, // Start halted
            command_rx: None,
            breakpoints: HashSet::new(),
        }
    }

    pub fn with_mcp(mut self, rx: CommandReceiver) -> Self {
        self.command_rx = Some(rx);
        self
    }

    /// Handle pending MCP commands (non-blocking)
    pub async fn handle_mcp_commands(&mut self, gba: &mut Gba) {
        // Collect all requests first to avoid borrow issues
        let mut requests = Vec::new();
        if let Some(rx) = &mut self.command_rx {
            while let Ok(req) = rx.try_recv() {
                requests.push(req);
            }
        }

        // Process requests
        for req in requests {
            let response = self.execute_command(req.command, gba);
            let _ = req.response_tx.send(response);
        }
    }

    fn execute_command(&mut self, command: DebugCommand, gba: &mut Gba) -> DebugResponse {
        match command {
            DebugCommand::Step { count } => {
                for _ in 0..count {
                    gba.step();
                }
                DebugResponse::StepComplete { instructions: count }
            }
            DebugCommand::Continue => {
                self.running = true;
                DebugResponse::ContinueStarted
            }
            DebugCommand::Halt => {
                self.running = false;
                let pc = gba.cpu.get_r(15);
                eprintln!("Execution halted at PC: 0x{:08X}", pc);
                DebugResponse::HaltComplete { pc }
            }
            DebugCommand::ReadMemory { address, length } => {
                let mut data = Vec::with_capacity(length as usize);
                for i in 0..length {
                    data.push(gba.mem.read_u8(address + i));
                }
                DebugResponse::MemoryData { address, data }
            }
            DebugCommand::ReadRegister { register } => {
                if register < 16 {
                    let value = gba.cpu.get_r(register);
                    DebugResponse::RegisterValue { register, value }
                } else {
                    DebugResponse::Error {
                        message: format!("Invalid register: {}", register),
                    }
                }
            }
            DebugCommand::GetCpuState => {
                let mut registers = [0u32; 16];
                for i in 0..16 {
                    registers[i as usize] = gba.cpu.get_r(i);
                }
                DebugResponse::CpuState {
                    registers,
                    cpsr: gba.cpu.get_cpsr(),
                    pc: gba.cpu.get_r(15),
                }
            }
            DebugCommand::GetPalette => {
                let mut data = Vec::with_capacity(256);
                for i in 0..256 {
                    data.push(gba.mem.read_u16(0x05000000 + i * 2));
                }
                DebugResponse::PaletteData { data }
            }
            DebugCommand::GetScreenshot => match self.encode_framebuffer_as_png(gba.framebuffer()) {
                Ok((width, height, rgba_data)) => DebugResponse::Screenshot { width, height, rgba_data },
                Err(e) => DebugResponse::Error {
                    message: format!("Failed to encode screenshot: {}", e),
                },
            },
            DebugCommand::Disassemble { address, count, mode } => {
                use gbae::system::instructions::{format_instruction_arm, format_instruction_thumb};

                let mut instructions = Vec::new();

                // Determine which mode to use for disassembly
                let is_thumb = match mode.as_deref() {
                    Some("thumb") => true,
                    Some("arm") => false,
                    Some("auto") | None => gba.cpu.get_thumb_state(),
                    _ => gba.cpu.get_thumb_state(), // Default to auto for invalid modes
                };

                if is_thumb {
                    // THUMB mode: 2 bytes per instruction
                    for i in 0..count {
                        let addr = address + i * 2;
                        let instruction = gba.mem.read_u16(addr);
                        let next = gba.mem.read_u16(addr + 2);
                        instructions.push(format_instruction_thumb(instruction, next, addr));
                    }
                } else {
                    // ARM mode: 4 bytes per instruction
                    for i in 0..count {
                        let addr = address + i * 4;
                        let instruction = gba.mem.read_u32(addr);
                        instructions.push(format_instruction_arm(instruction, addr));
                    }
                }

                DebugResponse::Disassembly { instructions }
            }
            DebugCommand::AddBreakpoint { address } => {
                self.breakpoints.insert(address);
                DebugResponse::BreakpointAdded { address }
            }
            DebugCommand::RemoveBreakpoint { address } => {
                if self.breakpoints.remove(&address) {
                    DebugResponse::BreakpointRemoved { address }
                } else {
                    DebugResponse::Error {
                        message: format!("No breakpoint at address 0x{:08X}", address),
                    }
                }
            }
            DebugCommand::ListBreakpoints => {
                let mut breakpoints: Vec<u32> = self.breakpoints.iter().copied().collect();
                breakpoints.sort();
                DebugResponse::BreakpointList { breakpoints }
            }
        }
    }

    pub fn has_breakpoints(&self) -> bool {
        !self.breakpoints.is_empty()
    }

    pub fn check_breakpoint(&mut self, pc: u32) -> bool {
        if self.breakpoints.contains(&pc) {
            self.running = false;
            eprintln!("Breakpoint hit at PC: 0x{:08X}", pc);
            true
        } else {
            false
        }
    }

    fn encode_framebuffer_as_png(&self, framebuffer: &Framebuffer) -> Result<(u32, u32, Vec<u8>), String> {
        use image::{ImageBuffer, ImageEncoder, Rgba};

        let width = framebuffer[0].len() as u32;
        let height = framebuffer.len() as u32;

        // Create image buffer
        let mut img_buffer = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(width, height);

        for (y, row) in framebuffer.iter().enumerate() {
            for (x, pixel) in row.iter().enumerate() {
                img_buffer.put_pixel(x as u32, y as u32, Rgba([pixel[0], pixel[1], pixel[2], 255]));
            }
        }

        // Encode as PNG
        let mut png_data = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut png_data);
        encoder
            .write_image(&img_buffer, width, height, image::ExtendedColorType::Rgba8)
            .map_err(|e| format!("PNG encoding failed: {}", e))?;

        Ok((width, height, png_data))
    }
}
