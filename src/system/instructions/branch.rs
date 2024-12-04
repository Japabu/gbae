use crate::{
    bitutil::{get_bit, get_bits, sign_extend}, 
    system::cpu::CPU,
    system::instructions::InstructionDecoder,
};

pub fn b_dec(instruction: u32) -> String {
    let link = get_bit(instruction, 24);
    let offset = get_bits(instruction, 0, 24);
    let target = ((offset as i32) << 8) >> 6; // Sign extend and multiply by 4
    
    format!("B{}{} #{:+}", 
        if link { "L" } else { "" },
        super::get_condition_code(instruction),
        target
    )
}

pub fn b(cpu: &mut CPU, instruction: u32) {
    let l = get_bit(instruction, 24);
    let signed_immed_24 = get_bits(instruction, 0, 24);
    if l {
        cpu.r[14] = cpu.next_instruction_address_from_execution_stage();
    }
    let offset = sign_extend(signed_immed_24, 24) << 2;
    cpu.r[15] = cpu.r[15].wrapping_add(offset);
}

pub fn bx(cpu: &mut CPU, instruction: u32) {
    assert_eq!(get_bits(instruction, 8, 12), 0b1111_1111_1111);

    let m = get_bits(instruction, 0, 4) as usize;
    let r_m = cpu.r[m];

    cpu.set_thumb_state(get_bit(r_m, 0));
    cpu.r[15] = r_m & 0xFFFFFFFE;
}
