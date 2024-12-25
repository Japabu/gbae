use crate::system::cpu::CPU;

pub struct Debugger {
    breakpoints: Vec<u32>,
    pub running: bool,
    step_mode: bool,
}

impl Debugger {
    pub fn new() -> Self {
        Self {
            breakpoints: Vec::new(),
            running: false,
            step_mode: false,
        }
    }

    pub fn add_breakpoint(&mut self, address: u32) {
        self.breakpoints.push(address);
    }

    pub fn should_break(&self, cpu: &CPU) -> bool {
        self.step_mode || self.breakpoints.contains(&cpu.get_r(15))
    }

    pub fn handle_command(&mut self, cpu: &mut CPU, command: &str) {
        let parts: Vec<&str> = command.trim().split_whitespace().collect();
        match parts.get(0).map(|s| *s) {
            Some("c") | Some("continue") => {
                self.running = true;
                self.step_mode = false;
            }
            Some("s") | Some("step") => {
                if let Some(n) = parts.get(1).and_then(|s| s.parse::<u32>().ok()) {
                    // Step n instructions
                    self.running = true;
                    self.step_mode = true;
                    for _ in 0..n - 1 {
                        cpu.cycle();
                    }
                } else {
                    // Single step
                    self.running = true;
                    self.step_mode = true;
                }
            }
            Some("b") | Some("break") => {
                if let Some(addr) = parts.get(1).and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()) {
                    self.add_breakpoint(addr);
                    println!("Breakpoint added at 0x{:08X}", addr);
                }
            }
            Some("p") | Some("print") => {
                cpu.print_registers();
                cpu.print_status();
            }
            Some("q") | Some("quit") => {
                std::process::exit(0);
            }
            Some("r") | Some("read") => {
                if let Some(addr) = parts.get(1).and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok()) {
                    println!("0x{:08X}: 0x{:08X}", addr, cpu.mem.read_u32(addr));
                }
            }
            Some("h") | Some("help") => {
                println!("Commands:");
                println!("  c/continue - Continue execution");
                println!("  s/step [n] - Step one or n instructions");
                println!("  b/break <addr> - Set breakpoint at address");
                println!("  p/print - Print CPU state");
                println!("  q/quit - Exit debugger");
                println!("  h/help - Show this help");
            }
            _ => println!("Unknown command. Type 'h' for help"),
        }
    }
}
