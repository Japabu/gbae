use crate::{
    bitutil::{get_bit, get_bits, sign_extend},
    system::CPU,
};

pub fn imm(cpu: &mut CPU, instruction: u32) {
    let l = get_bit(instruction, 24);
    let signed_immed_24 = get_bits(instruction, 0, 24);
    if l {
        cpu.r[14] = cpu.next_instruction_address_from_execution_stage();
    }
    let offset = sign_extend(signed_immed_24, 24) << 2;
    cpu.r[15] = cpu.r[15].wrapping_add(offset);
}
