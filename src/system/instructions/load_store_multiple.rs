use std::fmt::Display;

use crate::{
    bitutil::{get_bit, get_bit16, get_bits16, get_bits32},
    system::{
        cpu::{self, CPU, REGISTER_LR, REGISTER_PC, REGISTER_SP},
        memory::Memory,
    },
};

use super::{Condition, Instruction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadStoreMultiple {
    pub opcode: Opcode,
    pub n: u8,
    pub w: bool,
    pub s: bool,
    pub registers: u16,
    pub addressing_mode: AddressingMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    LDM,
    STM,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressingMode {
    DecrementAfter,
    IncrementAfter,
    DecrementBefore,
    IncrementBefore,
}

#[inline(always)]
pub fn decode_arm(instruction: u32) -> Instruction {
    Instruction::LoadStoreMultiple(LoadStoreMultiple {
        opcode: if get_bit(instruction, 20) { Opcode::LDM } else { Opcode::STM },
        n: get_bits32(instruction, 16, 4) as u8,
        w: get_bit(instruction, 21),
        s: get_bit(instruction, 22),
        registers: get_bits32(instruction, 0, 16) as u16,
        addressing_mode: match get_bits32(instruction, 23, 2) {
            0b00 => AddressingMode::DecrementAfter,
            0b01 => AddressingMode::IncrementAfter,
            0b10 => AddressingMode::DecrementBefore,
            _ => AddressingMode::IncrementBefore,
        },
    })
}

#[inline(always)]
pub fn decode_push_thumb(instruction: u16) -> Instruction {
    let is_lr = get_bits16(instruction, 8, 1);
    Instruction::LoadStoreMultiple(LoadStoreMultiple {
        opcode: Opcode::STM,
        n: REGISTER_SP,
        w: true,
        s: false,
        registers: get_bits16(instruction, 0, 8) | is_lr << REGISTER_LR,
        addressing_mode: AddressingMode::DecrementBefore,
    })
}

#[inline(always)]
pub fn decode_pop_thumb(instruction: u16) -> Instruction {
    let is_pc = get_bits16(instruction, 8, 1);
    Instruction::LoadStoreMultiple(LoadStoreMultiple {
        opcode: Opcode::LDM,
        n: REGISTER_SP,
        w: true,
        s: false,
        registers: get_bits16(instruction, 0, 8) | is_pc << REGISTER_PC,
        addressing_mode: AddressingMode::IncrementAfter,
    })
}

#[inline(always)]
pub fn decode_ldm_stm_thumb(instruction: u16) -> Instruction {
    let is_load = get_bit16(instruction, 11);
    let n = get_bits16(instruction, 8, 3) as u8;
    let registers = get_bits16(instruction, 0, 8);
    Instruction::LoadStoreMultiple(LoadStoreMultiple {
        opcode: if is_load { Opcode::LDM } else { Opcode::STM },
        n,
        w: !(is_load && get_bit16(registers, n)),
        s: false,
        registers,
        addressing_mode: AddressingMode::IncrementAfter,
    })
}

impl LoadStoreMultiple {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, mem: &mut Memory) {
        let (registers, size) = if self.registers == 0 { (1 << REGISTER_PC, 16 * 4) } else { (self.registers as u32, self.registers.count_ones() * 4) };
        let r_n = cpu.get_r(self.n);
        let start_address = match self.addressing_mode {
            AddressingMode::DecrementAfter => r_n.wrapping_sub(size).wrapping_add(4),
            AddressingMode::IncrementAfter => r_n,
            AddressingMode::DecrementBefore => r_n.wrapping_sub(size),
            AddressingMode::IncrementBefore => r_n.wrapping_add(4),
        };
        let new_base = match self.addressing_mode {
            AddressingMode::IncrementAfter | AddressingMode::IncrementBefore => r_n.wrapping_add(size),
            AddressingMode::DecrementAfter | AddressingMode::DecrementBefore => r_n.wrapping_sub(size),
        };

        let pc_in_list = get_bit(registers, REGISTER_PC);
        let user_bank = self.s && !(self.opcode == Opcode::LDM && pc_in_list);
        let mut address = start_address;
        match self.opcode {
            Opcode::LDM => {
                if self.w {
                    cpu.set_r(self.n, new_base);
                }
                for i in 0..REGISTER_PC {
                    if get_bit(registers, i) {
                        let value = mem.read_u32(address);
                        if user_bank {
                            cpu.set_r_in_mode(i, cpu::MODE_USR, value);
                        } else {
                            cpu.set_r(i, value);
                        }
                        address = address.wrapping_add(4);
                    }
                }
                if pc_in_list {
                    let value = mem.read_u32(address);
                    if self.s && cpu.current_mode_has_spsr() {
                        cpu.set_cpsr(cpu.get_spsr());
                    }
                    cpu.set_pc(value);
                }
            }
            Opcode::STM => {
                let first = registers.trailing_zeros() as u8;
                for i in 0..=REGISTER_PC {
                    if get_bit(registers, i) {
                        let value = if i == self.n && i != first && self.w {
                            new_base
                        } else if user_bank {
                            cpu.get_r_in_mode(i, cpu::MODE_USR)
                        } else if i == REGISTER_PC {
                            cpu.get_r(REGISTER_PC).wrapping_add(cpu.instruction_len_in_bytes())
                        } else {
                            cpu.get_r(i)
                        };
                        mem.write_u32(address, value);
                        address = address.wrapping_add(4);
                    }
                }
                if self.w {
                    cpu.set_r(self.n, new_base);
                }
            }
        }
    }

    pub fn disassemble(self, cond: Condition) -> String {
        let mut registers = String::new();
        for i in 0..=15 {
            if get_bit(self.registers as u32, i) {
                if !registers.is_empty() {
                    registers.push_str(", ");
                }
                registers.push_str(&format!("r{}", i));
            }
        }
        format!(
            "{:?}{}{} R{}{}, {{{}}}{}",
            self.opcode,
            cond,
            self.addressing_mode,
            self.n,
            if self.w { "!" } else { "" },
            registers,
            if self.s { "^" } else { "" }
        )
    }
}

impl Display for AddressingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AddressingMode::DecrementAfter => write!(f, "DA"),
            AddressingMode::IncrementAfter => write!(f, "IA"),
            AddressingMode::DecrementBefore => write!(f, "DB"),
            AddressingMode::IncrementBefore => write!(f, "IB"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pop_pc() {
        assert_eq!(Instruction::decode_thumb(0xBD10).disassemble(Condition::AL, 0), "LDMIA R13!, {r4, r15}");
    }

    #[test]
    fn test_stmfd_with_user_bank() {
        assert_eq!(Instruction::decode_arm(0xE96D4003).disassemble(Condition::AL, 0), "STMDB R13!, {r0, r1, r14}^");
    }
}
