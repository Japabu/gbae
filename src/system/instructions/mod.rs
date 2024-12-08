use std::fmt::Display;

use super::cpu::CPU;
use crate::bitutil::{get_bit, get_bits};

mod branch;
mod ctrl_ext;
mod data_processing;
mod ls;
mod lsm;
pub mod lut;

pub fn format_instruction(instruction: u32) -> String {
    format!(
        "{} ({:08x})\n\
            Bit Index:   27 26 25 24 23 22 21 20   07 06 05 04\n\
            Values:      {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<4} {:<2} {:<2} {:<2} {:<2}",
        lut::InstructionLut::decode(instruction),
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
    pub const fn decode_arm(instruction: u32) -> Condition {
        match get_bits(instruction, 28, 4) {
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
            0b1111 => panic!("Invalid condition"),
            _ => unreachable!(),
        }
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
            Condition::EQ => write!(f, "EQ"),
            Condition::NE => write!(f, "NE"),
            Condition::CS => write!(f, "CS"),
            Condition::CC => write!(f, "CC"),
            Condition::MI => write!(f, "MI"),
            Condition::PL => write!(f, "PL"),
            Condition::VS => write!(f, "VS"),
            Condition::VC => write!(f, "VC"),
            Condition::HI => write!(f, "HI"),
            Condition::LS => write!(f, "LS"),
            Condition::GE => write!(f, "GE"),
            Condition::LT => write!(f, "LT"),
            Condition::GT => write!(f, "GT"),
            Condition::LE => write!(f, "LE"),
            Condition::AL => Ok(()),
        }
    }
}

pub trait DecodedInstruction: Display {
    fn execute(&self, cpu: &mut CPU);
}

pub fn get_condition_code(instruction: u32) -> &'static str {
    todo!()
}
