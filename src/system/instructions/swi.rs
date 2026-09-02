use crate::{
    bitutil::{get_bits16, get_bits32},
    system::{
        cpu::{CPU, MODE_SVC, REGISTER_LR, REGISTER_PC},
        memory::Memory,
    },
};

use super::{Condition, Instruction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoftwareInterrupt {
    pub comment: u32,
}

#[inline(always)]
pub fn decode_arm(instruction: u32) -> Instruction {
    Instruction::SoftwareInterrupt(SoftwareInterrupt {
        comment: get_bits32(instruction, 0, 24),
    })
}

#[inline(always)]
pub fn decode_thumb(instruction: u16) -> Instruction {
    Instruction::SoftwareInterrupt(SoftwareInterrupt {
        comment: get_bits16(instruction, 0, 8) as u32,
    })
}

impl SoftwareInterrupt {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, _mem: &mut Memory) {
        let return_address = cpu.next_instruction_address_from_execution_stage();
        let old_cpsr = cpu.get_cpsr();

        cpu.set_mode(MODE_SVC);
        cpu.set_spsr(old_cpsr);
        cpu.set_r(REGISTER_LR, return_address);
        cpu.set_irq_disable(true);
        cpu.set_thumb_state(false);
        cpu.set_r(REGISTER_PC, 0x0000_0008);
    }

    pub fn disassemble(self, cond: Condition) -> String {
        format!("SWI{} #{:#X}", cond, self.comment)
    }
}
