use std::fmt::{Debug, Display};

use super::cpu::CPU;
use crate::bitutil::{get_bit, get_bits32};

mod branch;
mod ctrl_ext;
mod data_processing;
mod load_store;
mod load_store_multiple;
pub mod lut;

pub fn format_instruction_arm(instruction: u32, base_address: u32) -> String {
    format!(
        "{} ({:08X})\n\
            Bit Index:   27 26 25 24 23 22 21 20   07 06 05 04\n\
            Values:      {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<4} {:<2} {:<2} {:<2} {:<2}",
        lut::InstructionLut::decode_arm(instruction).disassemble(Condition::decode_arm(instruction), base_address),
        instruction,
        get_bit(instruction, 27) as u32,
        get_bit(instruction, 26) as u32,
        get_bit(instruction, 25) as u32,
        get_bit(instruction, 24) as u32,
        get_bit(instruction, 23) as u32,
        get_bit(instruction, 22) as u32,
        get_bit(instruction, 21) as u32,
        get_bit(instruction, 20) as u32,
        get_bit(instruction, 7) as u32,
        get_bit(instruction, 6) as u32,
        get_bit(instruction, 5) as u32,
        get_bit(instruction, 4) as u32,
    )
}

pub fn format_instruction_thumb(instruction: u16, next_instruction: u16, base_address: u32) -> String {
    format!(
        "{} ({:04X}, next: {:04X})\n\
            Bit Index:   15 14 13 12 11 10 09 08 07 06 05 04 03 02 01 00\n\
            Values:      {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2}",
        lut::InstructionLut::decode_thumb(instruction, next_instruction).disassemble(Condition::AL, base_address),
        instruction,
        next_instruction,
        get_bit(instruction as u32, 15) as u32,
        get_bit(instruction as u32, 14) as u32,
        get_bit(instruction as u32, 13) as u32,
        get_bit(instruction as u32, 12) as u32,
        get_bit(instruction as u32, 11) as u32,
        get_bit(instruction as u32, 10) as u32,
        get_bit(instruction as u32, 9) as u32,
        get_bit(instruction as u32, 8) as u32,
        get_bit(instruction as u32, 7) as u32,
        get_bit(instruction as u32, 6) as u32,
        get_bit(instruction as u32, 5) as u32,
        get_bit(instruction as u32, 4) as u32,
        get_bit(instruction as u32, 3) as u32,
        get_bit(instruction as u32, 2) as u32,
        get_bit(instruction as u32, 1) as u32,
        get_bit(instruction as u32, 0) as u32,
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    EQ, // Equal
    NE, // Not Equal
    CS, // Carry Set
    CC, // Carry Clear
    MI, // Minus
    PL, // Plus
    VS, // Overflow Set
    VC, // Overflow Clear
    HI, // Higher
    LS, // Lower or Same
    GE, // Greater or Equal
    LT, // Less Than
    GT, // Greater Than
    LE, // Less or Equal
    AL, // Always
}

impl Condition {
    pub const fn parse(cond: u8) -> Condition {
        match cond {
            0b0000 => Condition::EQ,
            0b0001 => Condition::NE,
            0b0010 => Condition::CS,
            0b0011 => Condition::CC,
            0b0100 => Condition::MI,
            0b0101 => Condition::PL,
            0b0110 => Condition::VS,
            0b0111 => Condition::VC,
            0b1000 => Condition::HI,
            0b1001 => Condition::LS,
            0b1010 => Condition::GE,
            0b1011 => Condition::LT,
            0b1100 => Condition::GT,
            0b1101 => Condition::LE,
            0b1110 => Condition::AL,
            _ => panic!("Invalid condition"),
        }
    }

    pub const fn decode_arm(instruction: u32) -> Condition {
        Condition::parse(get_bits32(instruction, 28, 4) as u8)
    }

    pub fn check(&self, cpu: &CPU) -> bool {
        match self {
            Condition::EQ => cpu.get_zero_flag(),
            Condition::NE => !cpu.get_zero_flag(),
            Condition::CS => cpu.get_carry_flag(),
            Condition::CC => !cpu.get_carry_flag(),
            Condition::MI => cpu.get_negative_flag(),
            Condition::PL => !cpu.get_negative_flag(),
            Condition::VS => cpu.get_overflow_flag(),
            Condition::VC => !cpu.get_overflow_flag(),
            Condition::HI => cpu.get_carry_flag() && !cpu.get_zero_flag(),
            Condition::LS => !cpu.get_carry_flag() || cpu.get_zero_flag(),
            Condition::GE => cpu.get_negative_flag() == cpu.get_overflow_flag(),
            Condition::LT => cpu.get_negative_flag() != cpu.get_overflow_flag(),
            Condition::GT => !cpu.get_zero_flag() && (cpu.get_negative_flag() == cpu.get_overflow_flag()),
            Condition::LE => cpu.get_zero_flag() || (cpu.get_negative_flag() != cpu.get_overflow_flag()),
            Condition::AL => true,
        }
    }
}

impl Display for Condition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Condition::AL => Ok(()),
            _ => write!(f, "{:?}", self),
        }
    }
}

pub trait DecodedInstruction: Debug {
    fn execute(&self, cpu: &mut CPU);
    fn disassemble(&self, cond: Condition, base_address: u32) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_arm() {
        assert_eq!(Condition::decode_arm(0b0000_0000_0000_0000_0000_0000_0000_0000), Condition::EQ);
        assert_eq!(Condition::decode_arm(0b0001_0000_0000_0000_0000_0000_0000_0000), Condition::NE);
        assert_eq!(Condition::decode_arm(0b0010_0000_0000_0000_0000_0000_0000_0000), Condition::CS);
        assert_eq!(Condition::decode_arm(0b0011_0000_0000_0000_0000_0000_0000_0000), Condition::CC);
        assert_eq!(Condition::decode_arm(0b0100_0000_0000_0000_0000_0000_0000_0000), Condition::MI);
        assert_eq!(Condition::decode_arm(0b0101_0000_0000_0000_0000_0000_0000_0000), Condition::PL);
        assert_eq!(Condition::decode_arm(0b0110_0000_0000_0000_0000_0000_0000_0000), Condition::VS);
        assert_eq!(Condition::decode_arm(0b0111_0000_0000_0000_0000_0000_0000_0000), Condition::VC);
        assert_eq!(Condition::decode_arm(0b1000_0000_0000_0000_0000_0000_0000_0000), Condition::HI);
        assert_eq!(Condition::decode_arm(0b1001_0000_0000_0000_0000_0000_0000_0000), Condition::LS);
        assert_eq!(Condition::decode_arm(0b1010_0000_0000_0000_0000_0000_0000_0000), Condition::GE);
        assert_eq!(Condition::decode_arm(0b1011_0000_0000_0000_0000_0000_0000_0000), Condition::LT);
        assert_eq!(Condition::decode_arm(0b1100_0000_0000_0000_0000_0000_0000_0000), Condition::GT);
        assert_eq!(Condition::decode_arm(0b1101_0000_0000_0000_0000_0000_0000_0000), Condition::LE);
        assert_eq!(Condition::decode_arm(0b1110_0000_0000_0000_0000_0000_0000_0000), Condition::AL);
        assert_eq!(Condition::decode_arm(0x39_00_00_00), Condition::CC);
    }
}
