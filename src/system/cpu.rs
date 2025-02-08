use std::{thread::sleep, time::Duration};

use crate::{
    bitutil::{get_bit, get_bits32, set_bit32, set_bits32},
    system::instructions::{format_instruction_arm, format_instruction_thumb},
};

use super::{
    instructions::{lut::InstructionLut, Condition},
    memory::Memory,
};

pub const MODE_USR: u8 = 0b10000;
pub const MODE_FIQ: u8 = 0b10001;
pub const MODE_IRQ: u8 = 0b10010;
pub const MODE_SVC: u8 = 0b10011;
pub const MODE_ABT: u8 = 0b10111;
pub const MODE_UND: u8 = 0b11011;
pub const MODE_SYS: u8 = 0b11111;

pub const REGISTER_SP: u8 = 13;
pub const REGISTER_LR: u8 = 14;
pub const REGISTER_PC: u8 = 15;

pub const INSTRUCTION_LEN_ARM: u32 = 4;
pub const INSTRUCTION_LEN_THUMB: u32 = 2;

pub const CPU_FREQUENCY: u64 = 16_776_000;
pub const INSTRUCTION_TIME: Duration = Duration::from_nanos(1_000_000_000 / CPU_FREQUENCY);

pub fn format_mode(mode: u8) -> &'static str {
    match mode {
        MODE_USR => "USR",
        MODE_FIQ => "FIQ",
        MODE_IRQ => "IRQ",
        MODE_SVC => "SVC",
        MODE_ABT => "ABT",
        MODE_UND => "UND",
        MODE_SYS => "SYS",
        _ => panic!("Invalid mode"),
    }
}

pub struct CPU {
    pub cpsr: u32, /* current program status register */

    /* r13: stack pointer, r14: link register, r15: pc */
    r: [u32; 16],    // unbanked
    r_svc: [u32; 2], // 13-14 banked in svc mode
    r_abt: [u32; 2], // 13-14 banked in abt mode
    r_und: [u32; 2], // 13-14 banked in und mode
    r_irq: [u32; 2], // 13-14 banked in irq mode
    r_fiq: [u32; 7], // 8-14  banked in fiq mode

    spsr_svc: u32,
    spsr_abt: u32,
    spsr_und: u32,
    spsr_irq: u32,
    spsr_fiq: u32,

    branch_happened: bool,
    cycles: u64,
}

impl CPU {
    pub fn get_r_in_mode(&self, r: u8, mode: u8) -> u32 {
        let banked_registers: &[u32] = match mode {
            MODE_USR | MODE_SYS => &[],
            MODE_SVC => &self.r_svc,
            MODE_ABT => &self.r_abt,
            MODE_UND => &self.r_und,
            MODE_IRQ => &self.r_irq,
            MODE_FIQ => &self.r_fiq,
            _ => panic!("Invalid mode"),
        };

        let banked_start = 15 - banked_registers.len() as u8;
        if r >= banked_start && r < 15 {
            banked_registers[(r - banked_start) as usize]
        } else {
            self.r[r as usize]
        }
    }

    pub fn get_r(&self, r: u8) -> u32 {
        self.get_r_in_mode(r, self.get_mode())
    }

    pub fn set_r_in_mode(&mut self, r: u8, mode: u8, value: u32) {
        let banked_registers: &mut [u32] = match mode {
            MODE_USR | MODE_SYS => &mut [],
            MODE_SVC => &mut self.r_svc,
            MODE_ABT => &mut self.r_abt,
            MODE_UND => &mut self.r_und,
            MODE_IRQ => &mut self.r_irq,
            MODE_FIQ => &mut self.r_fiq,
            _ => panic!("Invalid mode"),
        };

        let banked_start = 15 - banked_registers.len() as u8;
        if r >= banked_start && r < 15 {
            banked_registers[(r - banked_start) as usize] = value;
        } else {
            self.r[r as usize] = value;
        }

        if r == REGISTER_PC {
            self.branch_happened = true;
        }
    }

    pub fn set_r(&mut self, r: u8, value: u32) {
        self.set_r_in_mode(r, self.get_mode(), value)
    }

    pub fn get_cpsr(&self) -> u32 {
        self.cpsr
    }

    pub fn get_spsr(&self) -> u32 {
        match self.get_mode() {
            MODE_SVC => self.spsr_svc,
            MODE_ABT => self.spsr_abt,
            MODE_UND => self.spsr_und,
            MODE_IRQ => self.spsr_irq,
            MODE_FIQ => self.spsr_fiq,
            _ => panic!("Invalid mode"),
        }
    }

    pub fn set_spsr(&mut self, value: u32) {
        match self.get_mode() {
            MODE_SVC => self.spsr_svc = value,
            MODE_ABT => self.spsr_abt = value,
            MODE_UND => self.spsr_und = value,
            MODE_IRQ => self.spsr_irq = value,
            MODE_FIQ => self.spsr_fiq = value,
            _ => panic!("Invalid mode"),
        }
    }

    pub fn new() -> Self {
        InstructionLut::initialize();

        let mut cpu = CPU {
            cpsr: 0,

            r: [0; 16],    // unbanked
            r_svc: [0; 2], // banked in svc mode
            r_abt: [0; 2], // banked in abt mode
            r_und: [0; 2], // banked in und mode
            r_irq: [0; 2], // banked in irq mode
            r_fiq: [0; 7], // banked in fiq mode

            spsr_svc: 0,
            spsr_abt: 0,
            spsr_und: 0,
            spsr_irq: 0,
            spsr_fiq: 0,

            branch_happened: false,

            cycles: 0,
        };
        cpu.reset();
        cpu
    }

    pub fn cycle(&mut self, mem: &mut Memory) {
        let decoded_instruction = if self.get_thumb_state() {
            let instruction = self.fetch_thumb(mem);
            self.r[REGISTER_PC as usize] += self.instruction_len_in_bytes();
            InstructionLut::decode_thumb(instruction, self.fetch_thumb(mem))
        } else {
            let instruction = self.fetch_arm(mem);
            self.r[REGISTER_PC as usize] += self.instruction_len_in_bytes();
            let cond = Condition::decode_arm(instruction);
            if !cond.check(self) {
                return;
            }
            InstructionLut::decode_arm(instruction)
        };

        // Pc should be two instructions ahead of currently executed instruction
        self.r[REGISTER_PC as usize] += self.instruction_len_in_bytes();
        self.branch_happened = false;
        decoded_instruction.execute(self, mem);

        // If there was no branch set pc to the next instruction
        if !self.branch_happened {
            self.r[REGISTER_PC as usize] -= self.instruction_len_in_bytes();
        }

        // approximate cycle count for now
        self.cycles += 2;

        sleep(INSTRUCTION_TIME);
    }

    fn reset(&mut self) {
        self.set_mode(MODE_SVC);
        self.set_thumb_state(false);
        self.set_fiq_disable(true);
        self.set_irq_disable(true);
        self.r[REGISTER_PC as usize] = 0x00000000;
    }

    fn fetch_arm(&self, mem: &Memory) -> u32 {
        mem.read_u32(self.r[REGISTER_PC as usize])
    }

    fn fetch_thumb(&self, mem: &Memory) -> u16 {
        mem.read_u16(self.r[REGISTER_PC as usize])
    }

    fn fetch_next_thumb(&self, mem: &Memory) -> u16 {
        mem.read_u16(self.r[REGISTER_PC as usize] + INSTRUCTION_LEN_THUMB)
    }

    pub fn instruction_len_in_bytes(&self) -> u32 {
        if self.get_thumb_state() {
            INSTRUCTION_LEN_THUMB
        } else {
            INSTRUCTION_LEN_ARM
        }
    }

    pub fn next_instruction_address_from_execution_stage(&self) -> u32 {
        self.r[REGISTER_PC as usize] - self.instruction_len_in_bytes()
    }

    pub fn curr_instruction_address_from_execution_stage(&self) -> u32 {
        self.r[REGISTER_PC as usize] - self.instruction_len_in_bytes() * 2
    }

    pub fn get_negative_flag(&self) -> bool {
        get_bit(self.cpsr, 31)
    }
    pub fn set_negative_flag(&mut self, v: bool) {
        self.cpsr = set_bit32(self.cpsr, 31, v);
    }

    pub fn get_zero_flag(&self) -> bool {
        get_bit(self.cpsr, 30)
    }
    pub fn set_zero_flag(&mut self, v: bool) {
        self.cpsr = set_bit32(self.cpsr, 30, v);
    }
    pub fn get_carry_flag(&self) -> bool {
        get_bit(self.cpsr, 29)
    }
    pub fn set_carry_flag(&mut self, v: bool) {
        self.cpsr = set_bit32(self.cpsr, 29, v);
    }

    pub fn get_overflow_flag(&self) -> bool {
        get_bit(self.cpsr, 28)
    }
    pub fn set_overflow_flag(&mut self, v: bool) {
        self.cpsr = set_bit32(self.cpsr, 28, v);
    }

    pub fn get_irq_disable(&self) -> bool {
        get_bit(self.cpsr, 7)
    }
    pub fn set_irq_disable(&mut self, v: bool) {
        self.cpsr = set_bit32(self.cpsr, 7, v);
    }

    pub fn get_fiq_disable(&self) -> bool {
        get_bit(self.cpsr, 6)
    }
    pub fn set_fiq_disable(&mut self, v: bool) {
        self.cpsr = set_bit32(self.cpsr, 6, v);
    }

    pub fn get_thumb_state(&self) -> bool {
        get_bit(self.cpsr, 5)
    }
    pub fn set_thumb_state(&mut self, v: bool) {
        self.cpsr = set_bit32(self.cpsr, 5, v);
    }

    pub fn get_mode(&self) -> u8 {
        get_bits32(self.cpsr, 0, 5) as u8
    }
    pub fn set_mode(&mut self, v: u8) {
        self.cpsr = set_bits32(self.cpsr, 0, 5, v as u32);
    }
    pub fn current_mode_has_spsr(&self) -> bool {
        self.get_mode() != MODE_USR && self.get_mode() != MODE_SYS
    }
    pub fn in_a_privileged_mode(&self) -> bool {
        self.get_mode() != MODE_USR
    }
    pub fn get_cycles(&self) -> u64 {
        self.cycles
    }

    pub fn print_registers(&self) {
        for i in (0..16u8).step_by(4) {
            println!(
                "r{:2}: {:08X}   r{:2}: {:08X}   r{:2}: {:08X}   r{:2}: {:08X}",
                i,
                self.get_r(i),
                i + 1,
                self.get_r(i + 1),
                i + 2,
                self.get_r(i + 2),
                i + 3,
                self.get_r(i + 3),
            );
        }
    }

    pub fn print_status(&self) {
        println!(
            "CPSR: {:08X} [{}{}{}{}{}{}{}] MODE: {}",
            self.cpsr,
            if self.get_negative_flag() { 'N' } else { '-' },
            if self.get_zero_flag() { 'Z' } else { '-' },
            if self.get_carry_flag() { 'C' } else { '-' },
            if self.get_overflow_flag() { 'V' } else { '-' },
            if self.get_irq_disable() { 'I' } else { '-' },
            if self.get_fiq_disable() { 'F' } else { '-' },
            if self.get_thumb_state() { 'T' } else { '-' },
            format_mode(self.get_mode()),
        );
    }

    pub fn print_next_instruction(&self, mem: &Memory) {
        let pc = self.r[REGISTER_PC as usize];
        if self.get_thumb_state() {
            println!(
                "Next thumb instruction at {:08X}: {}",
                pc,
                format_instruction_thumb(self.fetch_thumb(mem), self.fetch_next_thumb(mem), pc)
            );
        } else {
            println!("Next arm instruction at {:08X}: {}", pc, format_instruction_arm(self.fetch_arm(mem), pc));
        }
    }
}
