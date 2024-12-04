use crate::{bitutil::{get_bit, get_bits}, system::cpu::CPU};

type AddressDecoderFn = fn(&mut CPU, u32) -> u32;
type LsHandlerFn = fn(&mut CPU, d: usize, address: u32);

pub fn addr_imm(cpu: &mut CPU, instruction: u32) -> u32 {
    let p = get_bit(instruction, 24);
    let u = get_bit(instruction, 23);
    let b = get_bit(instruction, 22);
    let w = get_bit(instruction, 21);
    let l = get_bit(instruction, 20);
    let offset_12 = get_bits(instruction, 0, 12);
    let n = get_bits(instruction, 16, 4) as usize;

    assert_eq!(p, true);
    assert_eq!(w, false);

    let r_n = cpu.r[n];

    if u {
        r_n.wrapping_add(offset_12)
    } else {
        r_n.wrapping_sub(offset_12)
    }
}

pub fn addr_reg(cpu: &mut CPU, instruction: u32) -> u32 {
    let p = get_bit(instruction, 24);
    let u = get_bit(instruction, 23);
    let b = get_bit(instruction, 22);
    let w = get_bit(instruction, 21);
    let l = get_bit(instruction, 20);
    let n = get_bits(instruction, 16, 4) as usize;
    let m = get_bits(instruction, 0, 4) as usize;

    assert_eq!(p, true);
    assert_eq!(w, false);
    assert_eq!(get_bits(instruction, 4, 8), 0);

    let r_n = cpu.r[n];
    let r_m = cpu.r[m];

    if u {
        r_n.wrapping_add(r_m)
    } else {
        r_n.wrapping_sub(r_m)
    }
}

pub fn handler(cpu: &mut CPU, instruction: u32, address_decoder: AddressDecoderFn, handler: LsHandlerFn) {
    let address = address_decoder(cpu, instruction);
    let d = get_bits(instruction, 12, 4) as usize;

    if d == 15 {
        panic!("ldr with destination register 15 not implemented");
    }

    handler(cpu, d, address);
}

pub fn ldr(cpu: &mut CPU, d: usize, address: u32) {
    cpu.r[d] = cpu.mem.read_u32(address as usize);
}

pub fn str(cpu: &mut CPU, d: usize, address: u32) {
    cpu.mem.write_u32(address as usize, cpu.r[d]);
}