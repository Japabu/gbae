use std::fmt::Display;

use crate::{
    bitutil::{get_bit, get_bits16, get_bits32},
    system::{
        cpu::{self, CPU, REGISTER_LR, REGISTER_PC, REGISTER_SP},
        memory::Memory,
    },
};

use super::{Condition, DecodedInstruction};

#[derive(Debug)]
struct LoadStoreMultiple {
    opcode: Opcode,
    addressing_mode: AddressingMode,
    s: bool,
}

#[derive(Debug)]
enum Opcode {
    LDM,
    STM,
}

#[derive(Debug)]
struct AddressingMode {
    n: u8,
    w: bool,
    registers: u16,
    typ: AddressingModeType,
}

#[derive(Debug)]
enum AddressingModeType {
    DecrementAfter,
    IncrementAfter,
    DecrementBefore,
    IncrementBefore,
}

pub fn decode_arm(instruction: u32) -> Box<dyn super::DecodedInstruction> {
    let registers = get_bits32(instruction, 0, 16) as u16;
    let n = get_bits32(instruction, 16, 4) as u8;
    let l = get_bit(instruction, 20);
    let w = get_bit(instruction, 21);
    let s = get_bit(instruction, 22);
    let pu = get_bits32(instruction, 23, 2) as u8;

    Box::new(LoadStoreMultiple {
        opcode: match l {
            true => Opcode::LDM,
            false => Opcode::STM,
        },
        addressing_mode: AddressingMode {
            n,
            w,
            registers,
            typ: match pu {
                0b00 => AddressingModeType::DecrementAfter,
                0b01 => AddressingModeType::IncrementAfter,
                0b10 => AddressingModeType::DecrementBefore,
                0b11 => AddressingModeType::IncrementBefore,
                _ => unreachable!(),
            },
        },
        s,
    })
}

pub fn decode_push_thumb(instruction: u16, _next_instruction: u16) -> Box<dyn super::DecodedInstruction> {
    let is_lr = get_bits16(instruction, 8, 1);
    let registers = get_bits16(instruction, 0, 8) | is_lr << REGISTER_LR;
    Box::new(LoadStoreMultiple {
        opcode: Opcode::STM,
        addressing_mode: AddressingMode {
            n: REGISTER_SP,
            w: true,
            registers,
            typ: AddressingModeType::DecrementBefore,
        },
        s: false,
    })
}

pub fn decode_pop_thumb(instruction: u16, _next_instruction: u16) -> Box<dyn super::DecodedInstruction> {
    let is_pc = get_bits16(instruction, 8, 1);
    let registers = get_bits16(instruction, 0, 8) | is_pc << REGISTER_PC;
    Box::new(LoadStoreMultiple {
        opcode: Opcode::LDM,
        addressing_mode: AddressingMode {
            n: REGISTER_SP,
            w: true,
            registers,
            typ: AddressingModeType::IncrementAfter,
        },
        s: false,
    })
}

impl DecodedInstruction for LoadStoreMultiple {
    fn execute(&self, cpu: &mut CPU, mem: &mut Memory) {
        let registers = self.addressing_mode.registers as u32;
        let (start_address, end_address) = self.addressing_mode.execute(cpu);

        let mut address = start_address;
        let cpu_mode = if self.s { cpu::MODE_USR } else { cpu.get_mode() };
        match self.opcode {
            Opcode::LDM => {
                if get_bit(registers, 15) {
                    todo!("ldm with destination register 15 not implemented");
                }
                for i in 0..=15 {
                    if get_bit(registers, i) {
                        cpu.set_r_in_mode(i, cpu_mode, mem.read_u32(address));
                        address += 4;
                    }
                }
            }
            Opcode::STM => {
                for i in 0..=15 {
                    if get_bit(registers, i) {
                        mem.write_u32(address, cpu.get_r_in_mode(i, cpu_mode));
                        address += 4;
                    }
                }
            }
        }
        assert_eq!(end_address, address - 4);
    }

    fn disassemble(&self, cond: Condition, _base_address: u32) -> String {
        // {LDM|STM}{<cond>}<addressing_mode>{^}
        format!("{:?}{}{}{}", self.opcode, cond, self.addressing_mode, if self.s { "^" } else { "" },)
    }
}

impl Display for AddressingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // {DA|IA|DB|IB} Rn{!}, registers

        write!(
            f,
            "{} R{}{}, {{",
            match self.typ {
                AddressingModeType::DecrementAfter => "DA",
                AddressingModeType::IncrementAfter => "IA",
                AddressingModeType::DecrementBefore => "DB",
                AddressingModeType::IncrementBefore => "IB",
            },
            self.n,
            if self.w { "!" } else { "" }
        )?;

        let mut is_first = true;
        for i in 0..=15 {
            if get_bit(self.registers as u32, i) {
                if !is_first {
                    write!(f, ", ")?;
                }
                is_first = false;
                write!(f, "r{}", i)?;
            }
        }
        write!(f, "}}")
    }
}

impl AddressingMode {
    pub fn execute(&self, cpu: &mut CPU) -> (u32, u32) {
        let r_n = cpu.get_r(self.n);
        let registers_count = self.registers.count_ones();
        let start_address = match self.typ {
            AddressingModeType::DecrementAfter => r_n - registers_count * 4 + 4,
            AddressingModeType::IncrementAfter => r_n,
            AddressingModeType::DecrementBefore => r_n - registers_count * 4,
            AddressingModeType::IncrementBefore => r_n + 4,
        };

        let end_address = match self.typ {
            AddressingModeType::DecrementAfter => r_n,
            AddressingModeType::IncrementAfter => r_n + registers_count * 4 - 4,
            AddressingModeType::DecrementBefore => r_n - 4,
            AddressingModeType::IncrementBefore => r_n + registers_count * 4,
        };

        if self.w {
            cpu.set_r(
                self.n,
                match self.typ {
                    AddressingModeType::DecrementAfter => r_n - registers_count * 4,
                    AddressingModeType::IncrementAfter => r_n + registers_count * 4,
                    AddressingModeType::DecrementBefore => r_n - registers_count * 4,
                    AddressingModeType::IncrementBefore => r_n + registers_count * 4,
                },
            );
        };

        (start_address, end_address)
    }
}
