use crate::{bitutil::{
    get_bit, get_bits, set_bit, set_bits,
}, system::instructions::format_instruction};

use super::{instructions::lut::InstructionLut, memory::Memory};

const MODE_USR: u32 = 0b10000;
const MODE_FIQ: u32 = 0b10001;
const MODE_IRQ: u32 = 0b10010;
const MODE_SVC: u32 = 0b10011;
const MODE_ABT: u32 = 0b10111;
const MODE_UND: u32 = 0b11011;
const MODE_SYS: u32 = 0b11111;

pub struct CPU {
    pub r: [u32; 16], /* r13: stack pointer, r14: link register, r15: pc */
    pub cpsr: u32,    /* current program status register */
    pub spsr: u32,    /* saved program status register */
    pub mem: Memory,
}

impl CPU {
    pub fn new(mem: Memory) -> Self {
        InstructionLut::initialize();

        let mut cpu = CPU {
            r: [0; 16],
            cpsr: 0,
            spsr: 0,
            mem,
        };
        cpu.reset();
        cpu
    }

    pub fn cycle(&mut self) {
        let instruction = self.fetch();
        self.advance_pc();

        if !self.evaluate_condition(instruction) {
            println!("Skipping: {}", format_instruction(instruction));
            return;
        }

        // Advance pc to two instructions because thats where it should be in the execution stage
        self.advance_pc();
        let pc_old = self.r[15];

        println!("Executing: {}", format_instruction(instruction));
        InstructionLut::get_handler(instruction)(self, instruction);

        // If there was no branch set pc to the next instruction
        if pc_old == self.r[15] {
            self.retreat_pc();
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
        println!("Fetching @ {:#x}", pc);
        if self.get_thumb_state() {
            self.mem.read_u16(pc) as u32
        } else {
            self.mem.read_u32(pc)
        }
    }

    fn evaluate_condition(&self, instruction: u32) -> bool {
        let condition = get_bits(instruction, 28, 4);
        match condition {
            0b0000 => self.get_zero_flag(),
            0b0010 => self.get_carry_flag(),
            0b0100 => self.get_negative_flag(),
            0b1110 => true,
            _ => panic!("Unknown condition: {:04b}", condition),
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
