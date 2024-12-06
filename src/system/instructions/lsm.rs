use crate::{
    bitutil::{get_bit, get_bits},
    system::{cpu::CPU, instructions::get_condition_code},
};

pub fn handler(cpu: &mut CPU, instruction: u32) {
    let (start_address, end_address) = addr(cpu, instruction);

    let is_load = get_bit(instruction, 20);
    if is_load {
        ldm(cpu, instruction, start_address, end_address);
    } else {
        stm(cpu, instruction, start_address, end_address);
    }
}

pub fn dec(instruction: u32) -> String {
    let addressing_mode = match get_bits(instruction, 23, 2) {
        0b00 => "DA",
        0b01 => "IA",
        0b10 => "DB",
        0b11 => "IB",
        _ => unreachable!(),
    };

    let is_load = get_bit(instruction, 20);
    let w_bit = get_bit(instruction, 21);
    let n = get_bits(instruction, 16, 4) as usize;
    let r_n = format!("r{}", n);

    let mut registers = Vec::new();
    for i in 0..16 {
        if get_bit(instruction, i) {
            registers.push(format!("r{}", i));
        }
    }

    format!(
        "{}{}{} {}{}, {{{}}}",
        if is_load { "LDM" } else { "STM" },
        get_condition_code(instruction),
        addressing_mode,
        r_n,
        if w_bit { "!" } else { "" },
        registers.join(", ")
    )
}

pub fn addr(cpu: &mut CPU, instruction: u32) -> (u32, u32) {
    let n = get_bits(instruction, 16, 4) as usize;
    let r_n = cpu.get_r(n);
    let register_list = get_bits(instruction, 0, 16);

    let mode = get_bits(instruction, 23, 2);
    let start_address = match mode {
        0b00 => r_n - register_list.count_ones() * 4 + 4,
        0b01 => r_n,
        0b10 => r_n - register_list.count_ones() * 4,
        0b11 => r_n + 4,
        _ => unreachable!(),
    };

    let end_address = match mode {
        0b00 => r_n,
        0b01 => r_n + register_list.count_ones() * 4 - 4,
        0b10 => r_n - 4,
        0b11 => r_n + register_list.count_ones() * 4,
        _ => unreachable!(),
    };

    let w_bit = get_bit(instruction, 21);
    if w_bit {
        cpu.set_r(
            n,
            match mode {
                0b00 => r_n - register_list.count_ones() * 4,
                0b01 => r_n + register_list.count_ones() * 4,
                0b10 => r_n - register_list.count_ones() * 4,
                0b11 => r_n + register_list.count_ones() * 4,
                _ => unreachable!(),
            },
        );
    };

    (start_address, end_address)
}

pub fn ldm(cpu: &mut CPU, instruction: u32, start_address: u32, end_address: u32) {
    assert_eq!(get_bit(instruction, 22), false);

    let register_list = get_bits(instruction, 0, 16);

    if get_bit(register_list, 15) {
        panic!("ldm with destination register 15 not implemented");
    }

    let mut address = start_address as usize;
    for i in 0..16 {
        if get_bit(instruction, i) {
            cpu.set_r(i as usize, cpu.mem.read_u32(address));
            address += 4;
        }
    }
    assert_eq!(end_address as usize, address - 4);
}

pub fn stm(cpu: &mut CPU, instruction: u32, start_address: u32, end_address: u32) {
    todo!("stm");
}
