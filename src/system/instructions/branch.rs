use crate::{
    bitutil::{get_bit, get_bits, sign_extend},
    system::{cpu::CPU, instructions::get_condition_code},
};

pub fn b(cpu: &mut CPU, instruction: u32) {
    let link = get_bit(instruction, 24);
    let signed_immed_24 = get_bits(instruction, 0, 24);
    let offset = sign_extend(signed_immed_24, 24) << 2;
    if link {
        cpu.set_r(14, cpu.next_instruction_address_from_execution_stage());
    }
    cpu.set_r(15, cpu.get_r(15).wrapping_add(offset));
}

pub fn b_dec(instruction: u32) -> String {
    let l = get_bit(instruction, 24);
    let signed_immed_24 = get_bits(instruction, 0, 24);
    let offset = (sign_extend(signed_immed_24, 24) << 2) + 8;

    format!(
        "B{}{} #{:+#x}",
        if l { "L" } else { "" },
        get_condition_code(instruction),
        offset
    )
}

pub fn bx(cpu: &mut CPU, instruction: u32) {
    assert_eq!(get_bits(instruction, 8, 12), 0b1111_1111_1111);

    let m = get_bits(instruction, 0, 4) as usize;
    let r_m = cpu.get_r(m);

    cpu.set_thumb_state(get_bit(r_m, 0));
    cpu.set_r(15, r_m & 0xFFFFFFFE);
}

pub fn bx_dec(instruction: u32) -> String {
    let m = get_bits(instruction, 0, 4) as usize;
    format!("BX{} r{}", get_condition_code(instruction), m)
}
