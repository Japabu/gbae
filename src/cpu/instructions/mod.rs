use crate::bitutil::get_bit;

use super::CPU;

mod dp;
mod branch;
mod ctrl_ext;
pub mod lut;

fn set_nz_flags(cpu: &mut CPU, value: u32) {
    cpu.set_negative_flag(get_bit(value, 31));
    cpu.set_zero_flag(value == 0);
}