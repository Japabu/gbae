mod registers;
mod instructions;

use instructions::lut::InstructionLut;
use registers::{Registers, MODE_SVC};

use crate::bitutil::{format_instruction, get_bits};

pub struct CPU {
    r: Registers,
}

impl CPU {
    pub fn new() -> Self {
        InstructionLut::initialize();

        let mut cpu = CPU {
            r: Registers::new(),
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
        self.r.set_mode(MODE_SVC);
        self.r.set_thumb_state(false);
        self.r.set_fiq_disable(true);
        self.r.set_irq_disable(true);
        self.r[15] = 0x00000000;
    }

    fn fetch(&self, mem: &[u8]) -> u32 {
        let pc = self.r[15] as usize;
        println!("Fetching @ {:#x}", pc);
        if self.r.get_thumb_state() {
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
            0b0000 => self.r.get_zero_flag(),
            0b0010 => self.r.get_carry_flag(),
            0b0100 => self.r.get_negative_flag(),
            0b1110 => true,
            _ => panic!("Unknown condition: {:04b}", condition),
        }
    }

    fn instruction_size_in_bytes(&self) -> u32 {
        if self.r.get_thumb_state() {
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
}
