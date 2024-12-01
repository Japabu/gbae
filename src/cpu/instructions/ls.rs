use crate::{bitutil::{get_bit, get_bits, read_u32}, cpu::CPU};

type AddressDecoderFn = fn(&mut CPU, u32) -> u32;
type LsHandlerFn = fn(&mut CPU, d: usize, address: u32);

pub fn addr_imm(cpu: &mut CPU, instruction: u32) -> u32 {
    let u = get_bit(instruction, 23);
    let offset_12 = get_bits(instruction, 0, 12);
    let n = get_bits(instruction, 16, 4) as usize;
    let r_n = cpu.r[n];

    if u {
        r_n.wrapping_add(offset_12)
    } else {
        r_n.wrapping_sub(offset_12)
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
    cpu.r[d] = read_u32(&cpu.mem, address as usize);
}

