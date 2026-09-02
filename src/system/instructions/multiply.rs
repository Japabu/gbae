use crate::{
    bitutil::{get_bit, get_bits16, get_bits32},
    system::cpu::CPU,
};

use super::{Condition, Instruction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Multiply {
    pub opcode: Opcode,
    pub set_flags: bool,
    pub d: u8,
    pub n: u8,
    pub s: u8,
    pub m: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    MUL,
    MLA,
    UMULL,
    UMLAL,
    SMULL,
    SMLAL,
}

#[inline(always)]
pub fn decode_arm(instruction: u32) -> Instruction {
    let long = get_bit(instruction, 23);
    let signed = get_bit(instruction, 22);
    let accumulate = get_bit(instruction, 21);
    Instruction::Multiply(Multiply {
        opcode: match (long, signed, accumulate) {
            (false, _, false) => Opcode::MUL,
            (false, _, true) => Opcode::MLA,
            (true, false, false) => Opcode::UMULL,
            (true, false, true) => Opcode::UMLAL,
            (true, true, false) => Opcode::SMULL,
            (true, true, true) => Opcode::SMLAL,
        },
        set_flags: get_bit(instruction, 20),
        d: get_bits32(instruction, 16, 4) as u8,
        n: get_bits32(instruction, 12, 4) as u8,
        s: get_bits32(instruction, 8, 4) as u8,
        m: get_bits32(instruction, 0, 4) as u8,
    })
}

#[inline(always)]
pub fn decode_mul_thumb(instruction: u16) -> Instruction {
    let d = get_bits16(instruction, 0, 3) as u8;
    Instruction::Multiply(Multiply {
        opcode: Opcode::MUL,
        set_flags: true,
        d,
        n: 0,
        s: d,
        m: get_bits16(instruction, 3, 3) as u8,
    })
}

impl Multiply {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU) {
        let r_m = cpu.get_r(self.m);
        let r_s = cpu.get_r(self.s);
        match self.opcode {
            Opcode::MUL | Opcode::MLA => {
                let mut result = r_m.wrapping_mul(r_s);
                if self.opcode == Opcode::MLA {
                    result = result.wrapping_add(cpu.get_r(self.n));
                }
                cpu.set_r(self.d, result);
                if self.set_flags {
                    cpu.set_nz(result);
                }
            }
            Opcode::UMULL | Opcode::UMLAL | Opcode::SMULL | Opcode::SMLAL => {
                let signed = matches!(self.opcode, Opcode::SMULL | Opcode::SMLAL);
                let accumulate = matches!(self.opcode, Opcode::UMLAL | Opcode::SMLAL);
                let mut result: u64 = if signed { (r_m as i32 as i64).wrapping_mul(r_s as i32 as i64) as u64 } else { (r_m as u64) * (r_s as u64) };
                if accumulate {
                    result = result.wrapping_add((cpu.get_r(self.d) as u64) << 32 | cpu.get_r(self.n) as u64);
                }
                cpu.set_r(self.n, result as u32);
                cpu.set_r(self.d, (result >> 32) as u32);
                if self.set_flags {
                    cpu.set_nz_flags(result >> 63 != 0, result == 0);
                }
            }
        }
    }

    pub fn disassemble(self, cond: Condition) -> String {
        let s = if self.set_flags { "S" } else { "" };
        match self.opcode {
            Opcode::MUL => format!("MUL{}{} R{}, R{}, R{}", cond, s, self.d, self.m, self.s),
            Opcode::MLA => format!("MLA{}{} R{}, R{}, R{}, R{}", cond, s, self.d, self.m, self.s, self.n),
            _ => format!("{:?}{}{} R{}, R{}, R{}, R{}", self.opcode, cond, s, self.n, self.d, self.m, self.s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multiply() {
        assert_eq!(Instruction::decode_arm(0xE0010392).disassemble(Condition::AL, 0), "MUL R1, R2, R3");
        assert_eq!(Instruction::decode_arm(0xE0C32190).disassemble(Condition::AL, 0), "SMULL R2, R3, R0, R1");
        assert_eq!(Instruction::decode_thumb(0x4348).disassemble(Condition::AL, 0), "MULS R0, R1, R0");
    }
}
