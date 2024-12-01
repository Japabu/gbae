use crate::cpu::{instructions::set_nz_flags, CPU};


pub fn add(cpu: &mut CPU, s: bool, n: u32, d: u32, so: u32, sco: bool) {
    let op1 = cpu.r[n];
    let (result, carry) = op1.overflowing_add(so);
    cpu.r[d] = result;

    if s {
        set_nz_flags(cpu, result);
        cpu.r.set_carry_flag(carry);
        // Set overflow flag if sign of both operands is different from result
        let overflow = (op1 ^ result) & (so ^ result) & 0x8000_0000 != 0;
        cpu.r.set_overflow_flag(overflow);
    }
}

pub fn and(cpu: &mut CPU, s: bool, n: u32, d: u32, so: u32, sco: bool) {
    cpu.r[d] = so & cpu.r[n];

    if s {
        set_nz_flags(cpu, cpu.r[d]);
        cpu.r.set_carry_flag(sco);
    }
}

pub fn mov(cpu: &mut CPU, s: bool, n: u32, d: u32, so: u32, sco: bool) {
    debug_assert_eq!(n, 0);

    cpu.r[d] = so;
    if s {
        set_nz_flags(cpu, cpu.r[d]);
        cpu.r.set_carry_flag(sco);
    }
}
