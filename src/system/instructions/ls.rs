use crate::{
    bitutil::{arithmetic_shift_right, get_bit, get_bits},
    system::{cpu::CPU, instructions::get_condition_code},
};

pub fn handler(cpu: &mut CPU, instruction: u32) {
    let d = get_bits(instruction, 12, 4) as usize;

    if d == 15 {
        panic!("ldr with destination register 15 not implemented");
    }

    let is_imm = !get_bit(instruction, 25);
    let address = if is_imm {
        addr_imm(cpu, instruction)
    } else {
        addr_reg(cpu, instruction)
    };

    let is_load = get_bit(instruction, 20);
    if is_load {
        ldr(cpu, d, address);
    } else {
        str(cpu, d, address);
    }
}

pub fn dec(instruction: u32) -> String {
    let d = get_bits(instruction, 12, 4) as usize;

    let is_imm = !get_bit(instruction, 25);
    let address = if is_imm {
        addr_imm_dec(instruction)
    } else {
        addr_reg_dec(instruction)
    };

    let is_load = get_bit(instruction, 20);
    format!(
        "{}{}{} r{}, {}",
        if is_load { "LDR" } else { "STR" },
        if get_bit(instruction, 22) { "B" } else { "" },
        get_condition_code(instruction),
        d,
        address
    )
}

fn addr_imm(cpu: &mut CPU, instruction: u32) -> u32 {
    let p = get_bit(instruction, 24);
    let u = get_bit(instruction, 23);
    let w = get_bit(instruction, 21);
    let offset_12 = get_bits(instruction, 0, 12);
    let n = get_bits(instruction, 16, 4) as usize;

    assert_eq!(p, true);
    assert_eq!(w, false);

    let r_n = cpu.get_r(n);

    if u {
        r_n.wrapping_add(offset_12)
    } else {
        r_n.wrapping_sub(offset_12)
    }
}

fn addr_imm_dec(instruction: u32) -> String {
    let u = get_bit(instruction, 23);
    let n = get_bits(instruction, 16, 4) as usize;
    let offset_12 = get_bits(instruction, 0, 12);
    format!("[r{}, #{}{:#x}]", n, if u { "+" } else { "-" }, offset_12)
}

fn addr_reg(cpu: &mut CPU, instruction: u32) -> u32 {
    let p = get_bit(instruction, 24);
    let u = get_bit(instruction, 23);
    let w = get_bit(instruction, 21);
    let n = get_bits(instruction, 16, 4) as usize;
    let m = get_bits(instruction, 0, 4) as usize;

    assert_eq!(p, true);
    assert_eq!(w, false);
    assert_eq!(get_bit(instruction, 4), false);

    let r_n = cpu.get_r(n);
    let r_m = cpu.get_r(m);

    let shift_imm = get_bits(instruction, 7, 5);
    let index = match get_bits(instruction, 5, 2) {
        0b00 => r_m << shift_imm,
        0b01 if shift_imm == 0 => 0,
        0b01 => r_m >> shift_imm,
        0b10 if shift_imm == 0 && get_bit(r_m, 31) => 0xFFFFFFFF,
        0b10 if shift_imm == 0 => 0,
        0b10 => arithmetic_shift_right(r_m, shift_imm),
        0b11 if shift_imm == 0 => (cpu.get_carry_flag() as u32) << 31 | r_m >> 1,
        0b11 => r_m.rotate_right(shift_imm),
        _ => unreachable!(),
    };

    if u {
        r_n.wrapping_add(index)
    } else {
        r_n.wrapping_sub(index)
    }
}

pub fn addr_reg_dec(instruction: u32) -> String {
    let u = get_bit(instruction, 23);
    let n = get_bits(instruction, 16, 4) as usize;
    let m = get_bits(instruction, 0, 4) as usize;
    let shift_imm = get_bits(instruction, 7, 5);
    let shift = match get_bits(instruction, 5, 2) {
        0b00 => "LSL",
        0b01 => "LSR",
        0b10 => "ASR",
        0b11 if shift_imm == 0 => "RRX",
        0b11 => "ROR",
        _ => unreachable!(),
    };

    if shift_imm == 0 {
        format!("[r{}, {}r{}]", n, if u { "+" } else { "-" }, m)
    } else {
        format!(
            "[r{}, {}r{}, {} #{}]",
            n,
            if u { "+" } else { "-" },
            m,
            shift,
            shift_imm
        )
    }
}

pub fn ldr(cpu: &mut CPU, d: usize, address: u32) {
    cpu.set_r(d, cpu.mem.read_u32(address as usize));
}

pub fn str(cpu: &mut CPU, d: usize, address: u32) {
    cpu.mem.write_u32(address as usize, cpu.get_r(d));
}
