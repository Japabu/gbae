use crate::bitutil::get_bit;

use super::cpu::CPU;

mod dp;
mod branch;
mod ctrl_ext;
mod ls;
pub mod lut;

fn set_nz_flags(cpu: &mut CPU, value: u32) {
    cpu.set_negative_flag(get_bit(value, 31));
    cpu.set_zero_flag(value == 0);
}

pub fn format_instruction(instruction: u32) -> String {
    let mnemonic = lut::InstructionLut::get_decoder(instruction);
    format!(
        "{} ({:08x})\n\
        Bit Index:   27 26 25 24 23 22 21 20 07 06 05 04\n\
        Values:      {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2}\n",
        mnemonic,
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