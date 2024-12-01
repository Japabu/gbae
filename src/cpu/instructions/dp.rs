use crate::cpu::{instructions::set_nz_flags, CPU};


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