mod instructions;

use instructions::lut::InstructionLut;

use crate::bitutil::{format_instruction, get_bit, get_bits, set_bit, set_bits};

const MODE_USR: u32 = 0b10000;
const MODE_FIQ: u32 = 0b10001;
const MODE_IRQ: u32 = 0b10010;
const MODE_SVC: u32 = 0b10011;
const MODE_ABT: u32 = 0b10111;
const MODE_UND: u32 = 0b11011;
const MODE_SYS: u32 = 0b11111;

pub struct CPU {
    pub r: [u32; 16],   /* r13: stack pointer, r14: link register, r15: pc */
    pub cpsr: u32,      /* current program status register */
    pub spsr: u32,      /* saved program status register */
}

impl CPU {
    pub fn new() -> Self {
        InstructionLut::initialize();

        let mut cpu = CPU {
            r: [0; 16],
            cpsr: 0,
            spsr: 0,
        };
        cpu.reset();
        cpu
    }

    pub fn cycle(&mut self, mem: &mut [u8]) {
        let instruction = self.fetch(mem);
        self.advance_pc();

        if !self.evaluate_condition(instruction) {
            println!("Skipping: {}", format_instruction(instruction));
            return;
        }


        // Advance pc to two instructions because thats where it should be in the execution stage
        self.advance_pc();
        let pc_old = self.r[15];

        println!("Executing: {}", format_instruction(instruction));
        InstructionLut::get(instruction)(self, instruction);

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

    fn fetch(&self, mem: &[u8]) -> u32 {
        let pc = self.r[15] as usize;
        println!("Fetching @ {:#x}", pc);
        if self.get_thumb_state() {
            u16::from_le_bytes(
                mem[pc..pc + 2]
                    .try_into()
                    .expect("Failed to fetch thumb instruction"),
            ) as u32
        } else {
            u32::from_le_bytes(
                mem[pc..pc + 4]
                    .try_into()
                    .expect("Failed to fetch arm instruction"),
            )
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

    fn next_instruction_address_from_execution_stage(&self) -> u32 {
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
