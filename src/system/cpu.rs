use crate::{
    bitutil::{get_bit, get_bits, set_bit, set_bits},
    system::instructions::format_instruction,
};

use super::{
    instructions::{self, lut::InstructionLut},
    memory::Memory,
};

const MODE_USR: u32 = 0b10000;
const MODE_FIQ: u32 = 0b10001;
const MODE_IRQ: u32 = 0b10010;
const MODE_SVC: u32 = 0b10011;
const MODE_ABT: u32 = 0b10111;
const MODE_UND: u32 = 0b11011;
const MODE_SYS: u32 = 0b11111;

pub struct CPU {
    pub cpsr: u32, /* current program status register */
    pub mem: Memory,

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
}

impl CPU {
    pub fn get_r(&self, r: usize) -> u32 {
        let banked_registers: &[u32] = match self.get_mode() {
            MODE_USR | MODE_SYS => &self.r,
            MODE_SVC => &self.r_svc,
            MODE_ABT => &self.r_abt,
            MODE_UND => &self.r_und,
            MODE_IRQ => &self.r_irq,
            MODE_FIQ => &self.r_fiq,
            _ => panic!("Invalid mode"),
        };

        let banked_start = 15 - banked_registers.len();
        if r >= banked_start && r < 15 {
            banked_registers[r - banked_start]
        } else {
            self.r[r]
        }
    }

    pub fn set_r(&mut self, r: usize, value: u32) {
        let banked_registers: &mut [u32] = match self.get_mode() {
            MODE_USR | MODE_SYS => &mut [],
            MODE_SVC => &mut self.r_svc,
            MODE_ABT => &mut self.r_abt,
            MODE_UND => &mut self.r_und,
            MODE_IRQ => &mut self.r_irq,
            MODE_FIQ => &mut self.r_fiq,
            _ => panic!("Invalid mode"),
        };

        let banked_start = 15 - banked_registers.len();
        if r >= banked_start && r < 15 {
            banked_registers[r - banked_start] = value;
        } else {
            self.r[r] = value;
        }
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

    pub fn new(mem: Memory) -> Self {
        InstructionLut::initialize();

        let mut cpu = CPU {
            cpsr: 0,
            mem,

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
        };
        cpu.reset();
        cpu
    }

    pub fn cycle(&mut self) {
        if self.get_thumb_state() {
            panic!("Thumb mode not implemented");
        }

        let instruction = self.fetch();
        self.advance_pc();

        if !instructions::evaluate_condition(&self, instruction) {
            //println!("Skipping: {}", format_instruction(instruction));
            return;
        }

        // Advance pc to two instructions because thats where it should be in the execution stage
        self.advance_pc();
        let pc_old = self.r[15];

        //self.print_registers();
        //println!("Executing: {}", format_instruction(instruction));
        InstructionLut::get_handler(instruction)(self, instruction);

        // If there was no branch set pc to the next instruction
        if pc_old == self.r[15] {
            self.retreat_pc();
        }
    }

    pub fn peek_next_instruction(&self) -> u32 {
        let pc = self.r[15] as usize;
        if self.get_thumb_state() {
            self.mem.read_u16(pc) as u32
        } else {
            self.mem.read_u32(pc)
        }
    }

    fn reset(&mut self) {
        self.set_mode(MODE_SVC);
        self.set_thumb_state(false);
        self.set_fiq_disable(true);
        self.set_irq_disable(true);
        self.r[15] = 0x00000000;
    }

    fn fetch(&mut self) -> u32 {
        let pc = self.r[15] as usize;
        //println!("Fetching @ {:#x}", pc);
        if self.get_thumb_state() {
            self.mem.read_u16(pc) as u32
        } else {
            self.mem.read_u32(pc)
        }
    }

    fn instruction_size_in_bytes(&self) -> u32 {
        if self.get_thumb_state() {
            2
        } else {
            4
        }
    }

    fn advance_pc(&mut self) {
        self.r[15] += self.instruction_size_in_bytes()
    }
    fn retreat_pc(&mut self) {
        self.r[15] -= self.instruction_size_in_bytes()
    }

    pub fn next_instruction_address_from_execution_stage(&self) -> u32 {
        self.r[15] - self.instruction_size_in_bytes()
    }

    pub fn get_negative_flag(&self) -> bool {
        get_bit(self.cpsr, 31)
    }
    pub fn set_negative_flag(&mut self, v: bool) {
        self.cpsr = set_bit(self.cpsr, 31, v);
    }

    pub fn get_zero_flag(&self) -> bool {
        get_bit(self.cpsr, 30)
    }
    pub fn set_zero_flag(&mut self, v: bool) {
        self.cpsr = set_bit(self.cpsr, 30, v);
    }
    pub fn get_carry_flag(&self) -> bool {
        get_bit(self.cpsr, 29)
    }
    pub fn set_carry_flag(&mut self, v: bool) {
        self.cpsr = set_bit(self.cpsr, 29, v);
    }

    pub fn get_overflow_flag(&self) -> bool {
        get_bit(self.cpsr, 28)
    }
    pub fn set_overflow_flag(&mut self, v: bool) {
        self.cpsr = set_bit(self.cpsr, 28, v);
    }

    pub fn get_irq_disable(&self) -> bool {
        get_bit(self.cpsr, 7)
    }
    pub fn set_irq_disable(&mut self, v: bool) {
        self.cpsr = set_bit(self.cpsr, 7, v);
    }

    pub fn get_fiq_disable(&self) -> bool {
        get_bit(self.cpsr, 6)
    }
    pub fn set_fiq_disable(&mut self, v: bool) {
        self.cpsr = set_bit(self.cpsr, 6, v);
    }

    pub fn get_thumb_state(&self) -> bool {
        get_bit(self.cpsr, 5)
    }
    pub fn set_thumb_state(&mut self, v: bool) {
        self.cpsr = set_bit(self.cpsr, 5, v);
    }

    pub fn get_mode(&self) -> u32 {
        get_bits(self.cpsr, 0, 5)
    }
    pub fn set_mode(&mut self, v: u32) {
        self.cpsr = set_bits(self.cpsr, 0, 5, v);
    }
    pub fn current_mode_has_spsr(&self) -> bool {
        self.get_mode() != MODE_USR && self.get_mode() != MODE_SYS
    }
    pub fn in_a_privileged_mode(&self) -> bool {
        self.get_mode() != MODE_USR
    }
}
