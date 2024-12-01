use crate::bitutil::get_bit;

use super::CPU;

mod dp;
mod branch;
pub mod lut;

fn set_nz_flags(cpu: &mut CPU, value: u32) {
    cpu.r.set_negative_flag(get_bit(value, 31));
    cpu.r.set_zero_flag(value == 0);
}