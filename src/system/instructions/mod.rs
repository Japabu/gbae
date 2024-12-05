use super::cpu::CPU;
use crate::bitutil::{get_bit, get_bits};

mod branch;
mod ctrl_ext;
mod dp;
mod ls;
pub mod lut;

fn set_nz_flags(cpu: &mut CPU, value: u32) {
    cpu.set_negative_flag(get_bit(value, 31));
    cpu.set_zero_flag(value == 0);
}

pub fn get_condition_code(instruction: u32) -> &'static str {
    match get_bits(instruction, 28, 4) {
        0b0000 => "EQ", // Equal
        0b0001 => "NE", // Not Equal
        0b0010 => "CS", // Carry Set
        0b0011 => "CC", // Carry Clear
        0b0100 => "MI", // Minus
        0b0101 => "PL", // Plus
        0b0110 => "VS", // Overflow Set
        0b0111 => "VC", // Overflow Clear
        0b1000 => "HI", // Higher
        0b1001 => "LS", // Lower or Same
        0b1010 => "GE", // Greater or Equal
        0b1011 => "LT", // Less Than
        0b1100 => "GT", // Greater Than
        0b1101 => "LE", // Less or Equal
        0b1110 => "",   // Always
        0b1111 => "NV", // Never
        _ => unreachable!(),
    }
}

pub(crate) fn evaluate_condition(cpu: &CPU, instruction: u32) -> bool {
    let condition = get_bits(instruction, 28, 4);
    match condition {
        0b0000 => cpu.get_zero_flag(),
        0b0010 => cpu.get_carry_flag(),
        0b0100 => cpu.get_negative_flag(),
        0b1011 => cpu.get_negative_flag() != cpu.get_overflow_flag(),
        0b1110 => true,
        _ => panic!("Unknown condition: {:04b}", condition),
    }
}

pub fn format_instruction(instruction: u32) -> String {
    format!(
        "{} ({:08x})\n\
            Bit Index:   27 26 25 24 23 22 21 20   07 06 05 04\n\
            Values:      {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<4} {:<2} {:<2} {:<2} {:<2}",
        lut::InstructionLut::get_decoder(instruction),
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
