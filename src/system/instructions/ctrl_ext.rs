pub mod mrs {
    use crate::{
        bitutil::{get_bit, get_bits32},
        system::{
            cpu::CPU,
            instructions::{Condition, DecodedInstruction},
        },
    };

    #[derive(Debug)]
    struct Mrs {
        d: u8,
        r: bool,
    }

    pub fn decode_arm(instruction: u32) -> Box<dyn DecodedInstruction> {
        Box::new(Mrs {
            d: get_bits32(instruction, 12, 4) as u8,
            r: get_bit(instruction, 22),
        })
    }

    impl DecodedInstruction for Mrs {
        fn execute(&self, cpu: &mut CPU) {
            if self.r {
                cpu.set_r(self.d, cpu.get_spsr());
            } else {
                cpu.set_r(self.d, cpu.get_cpsr());
            }
        }

        fn disassemble(&self, cond: Condition) -> String {
            // MRS{<cond>} <Rd>, <CPSR|SPSR>
            format!("MRS{} R{}, {}", cond, self.d, if self.r { "SPSR" } else { "CPSR" })
        }
    }
}

pub mod msr {
    use crate::{
        bitutil::{get_bit, get_bits32},
        system::{
            cpu::CPU,
            instructions::{Condition, DecodedInstruction},
        },
    };

    // Masks for processor ARM7TDMI
    const UNALLOC_MASK: u32 = 0x0FFFFF00;
    const USER_MASK: u32 = 0xF0000000;
    const PRIV_MASK: u32 = 0x0000000F;
    const STATE_MASK: u32 = 0x00000020;

    #[derive(Debug)]
    struct Msr {
        mode: MsrOperand,
        field_mask: u8,
        r: bool,
    }

    #[derive(Debug)]
    enum MsrOperand {
        Immediate(u32),
        Register(u8),
    }

    pub fn decode_arm(instruction: u32) -> Box<dyn DecodedInstruction> {
        debug_assert_eq!(get_bits32(instruction, 12, 4), 0b1111);

        let is_immediate = get_bit(instruction, 25);
        Box::new(Msr {
            mode: match is_immediate {
                false => MsrOperand::Register(get_bits32(instruction, 0, 4) as u8),
                true => MsrOperand::Immediate((get_bits32(instruction, 0, 8)).rotate_right(get_bits32(instruction, 8, 4))),
            },
            field_mask: get_bits32(instruction, 16, 4) as u8,
            r: get_bit(instruction, 22),
        })
    }

    impl DecodedInstruction for Msr {
        fn execute(&self, cpu: &mut CPU) {
            let operand = match self.mode {
                MsrOperand::Immediate(imm) => imm,
                MsrOperand::Register(m) => cpu.get_r(m),
            };

            if operand & UNALLOC_MASK != 0 {
                panic!("Attempt to set reserved bits");
            }

            let mut mask = 0u32;
            for i in 0..4 {
                if get_bit(self.field_mask as u32, i) {
                    mask |= 0xFF << (8 * i);
                }
            }

            if !self.r {
                if cpu.in_a_privileged_mode() {
                    if operand & STATE_MASK != 0 {
                        panic!("Attempt to set non-ARM execution state");
                    } else {
                        mask &= USER_MASK | PRIV_MASK;
                    }
                } else {
                    mask &= USER_MASK;
                }
                cpu.cpsr = (cpu.cpsr & !mask) | (operand & mask);
            } else {
                if cpu.current_mode_has_spsr() {
                    mask &= USER_MASK | PRIV_MASK | STATE_MASK;
                    cpu.set_spsr((cpu.get_spsr() & !mask) | (operand & mask));
                } else {
                    panic!("Tried to set SPSR in user or system mode");
                }
            }
        }
        fn disassemble(&self, cond: Condition) -> String {
            // MSR{<cond>} {CPSR|SPSR}_<fields>, <#immediate|Rm>
            let field_mask = self.field_mask as u32;
            format!(
                "MSR{} {}_{}{}{}{}, {}",
                cond,
                if self.r { "SPSR" } else { "CPSR" },
                if get_bit(field_mask, 0) { "c" } else { "" },
                if get_bit(field_mask, 1) { "x" } else { "" },
                if get_bit(field_mask, 2) { "s" } else { "" },
                if get_bit(field_mask, 3) { "f" } else { "" },
                match self.mode {
                    MsrOperand::Immediate(imm) => format!("#{:08X}", imm),
                    MsrOperand::Register(m) => format!("R{}", m),
                }
            )
        }
    }
}
