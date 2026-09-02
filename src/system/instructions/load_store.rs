use std::fmt::Display;

use crate::{
    bitutil::{get_bit, get_bit16, get_bits16, get_bits32, sign_extend32},
    system::{
        cpu::{CPU, REGISTER_PC, REGISTER_SP},
        memory::{Access, Memory},
    },
};

use super::{
    data_processing::{Shift, ShifterOperand},
    Condition, Instruction,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadStore {
    pub opcode: Opcode,
    pub length: Length,
    pub sign_extend: bool,
    pub d: u8,
    pub addressing_mode: AddressingMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swap {
    pub byte: bool,
    pub d: u8,
    pub m: u8,
    pub n: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    LDR,
    STR,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Length {
    Byte,
    Halfword,
    Word,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddressingMode {
    pub n: u8,
    pub u_is_add: bool,
    pub offset: ShifterOperand,
    pub indexing_mode: IndexingMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexingMode {
    Offset,
    PreIndexed,
    PostIndexed,
}

#[inline(always)]
pub fn decode_arm(instruction: u32) -> Instruction {
    Instruction::LoadStore(LoadStore {
        opcode: if get_bit(instruction, 20) { Opcode::LDR } else { Opcode::STR },
        length: if get_bit(instruction, 22) { Length::Byte } else { Length::Word },
        sign_extend: false,
        d: get_bits32(instruction, 12, 4) as u8,
        addressing_mode: AddressingMode::decode_arm(instruction),
    })
}

#[inline(always)]
pub fn decode_extra_arm(instruction: u32) -> Instruction {
    let l = get_bit(instruction, 20);
    let s = get_bit(instruction, 6);
    let h = get_bit(instruction, 5);
    let (opcode, sign_extend, length) = match (l, s, h) {
        (false, false, true) => (Opcode::STR, false, Length::Halfword),
        (true, false, true) => (Opcode::LDR, false, Length::Halfword),
        (true, true, false) => (Opcode::LDR, true, Length::Byte),
        (true, true, true) => (Opcode::LDR, true, Length::Halfword),
        _ => return Instruction::Unknown(instruction),
    };
    Instruction::LoadStore(LoadStore {
        opcode,
        length,
        sign_extend,
        d: get_bits32(instruction, 12, 4) as u8,
        addressing_mode: AddressingMode::decode_extra_arm(instruction),
    })
}

#[inline(always)]
pub fn decode_swap_arm(instruction: u32) -> Instruction {
    Instruction::Swap(Swap {
        byte: get_bit(instruction, 22),
        d: get_bits32(instruction, 12, 4) as u8,
        m: get_bits32(instruction, 0, 4) as u8,
        n: get_bits32(instruction, 16, 4) as u8,
    })
}

#[inline(always)]
fn thumb_offset(n: u8, offset: ShifterOperand) -> AddressingMode {
    AddressingMode {
        n,
        u_is_add: true,
        offset,
        indexing_mode: IndexingMode::Offset,
    }
}

#[inline(always)]
fn thumb_immediate(n: u8, immed: u16) -> AddressingMode {
    thumb_offset(n, ShifterOperand::Immediate { immed, rotate_imm: 0 })
}

#[inline(always)]
pub fn decode_halfword_thumb(instruction: u16) -> Instruction {
    Instruction::LoadStore(LoadStore {
        opcode: if get_bit16(instruction, 11) { Opcode::LDR } else { Opcode::STR },
        length: Length::Halfword,
        sign_extend: false,
        d: get_bits16(instruction, 0, 3) as u8,
        addressing_mode: thumb_immediate(get_bits16(instruction, 3, 3) as u8, get_bits16(instruction, 6, 5) * 2),
    })
}

#[inline(always)]
pub fn decode_word_byte_thumb(instruction: u16) -> Instruction {
    let offset = get_bits16(instruction, 6, 5);
    let is_byte = get_bit16(instruction, 12);
    Instruction::LoadStore(LoadStore {
        opcode: if get_bit16(instruction, 11) { Opcode::LDR } else { Opcode::STR },
        length: if is_byte { Length::Byte } else { Length::Word },
        sign_extend: false,
        d: get_bits16(instruction, 0, 3) as u8,
        addressing_mode: thumb_immediate(get_bits16(instruction, 3, 3) as u8, if is_byte { offset } else { offset * 4 }),
    })
}

#[inline(always)]
pub fn decode_stack_thumb(instruction: u16) -> Instruction {
    Instruction::LoadStore(LoadStore {
        opcode: if get_bit16(instruction, 11) { Opcode::LDR } else { Opcode::STR },
        length: Length::Word,
        sign_extend: false,
        d: get_bits16(instruction, 8, 3) as u8,
        addressing_mode: thumb_immediate(REGISTER_SP, get_bits16(instruction, 0, 8) * 4),
    })
}

#[inline(always)]
pub fn decode_load_from_literal_pool_thumb(instruction: u16) -> Instruction {
    Instruction::LoadStore(LoadStore {
        opcode: Opcode::LDR,
        length: Length::Word,
        sign_extend: false,
        d: get_bits16(instruction, 8, 3) as u8,
        addressing_mode: thumb_immediate(REGISTER_PC, get_bits16(instruction, 0, 8) * 4),
    })
}

#[inline(always)]
pub fn decode_register_offset_thumb(instruction: u16) -> Instruction {
    let (opcode, sign_extend, length) = match get_bits16(instruction, 9, 3) {
        0b000 => (Opcode::STR, false, Length::Word),
        0b001 => (Opcode::STR, false, Length::Halfword),
        0b010 => (Opcode::STR, false, Length::Byte),
        0b011 => (Opcode::LDR, true, Length::Byte),
        0b100 => (Opcode::LDR, false, Length::Word),
        0b101 => (Opcode::LDR, false, Length::Halfword),
        0b110 => (Opcode::LDR, false, Length::Byte),
        _ => (Opcode::LDR, true, Length::Halfword),
    };
    Instruction::LoadStore(LoadStore {
        opcode,
        length,
        sign_extend,
        d: get_bits16(instruction, 0, 3) as u8,
        addressing_mode: thumb_offset(
            get_bits16(instruction, 3, 3) as u8,
            ShifterOperand::Register {
                m: get_bits16(instruction, 6, 3) as u8,
            },
        ),
    })
}

impl LoadStore {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, mem: &mut Memory) {
        let value = cpu.get_r(self.d).wrapping_add(if self.d == REGISTER_PC { cpu.instruction_len_in_bytes() } else { 0 });
        let address = self.addressing_mode.execute(cpu);
        match (self.opcode, self.length, self.sign_extend) {
            (Opcode::LDR, Length::Word, _) => {
                let value = mem.load_u32(address, Access::Nonsequential).rotate_right((address & 0b11) * 8);
                if self.d == REGISTER_PC {
                    cpu.set_pc(value);
                } else {
                    cpu.set_r(self.d, value);
                }
            }
            (Opcode::LDR, Length::Halfword, false) => cpu.set_r(self.d, (mem.load_u16(address, Access::Nonsequential) as u32).rotate_right((address & 0b1) * 8)),
            (Opcode::LDR, Length::Halfword, true) => {
                let value = if address & 0b1 != 0 {
                    sign_extend32(mem.load_u8(address, Access::Nonsequential) as u32, 8)
                } else {
                    sign_extend32(mem.load_u16(address, Access::Nonsequential) as u32, 16)
                };
                cpu.set_r(self.d, value);
            }
            (Opcode::LDR, Length::Byte, false) => cpu.set_r(self.d, mem.load_u8(address, Access::Nonsequential) as u32),
            (Opcode::LDR, Length::Byte, true) => cpu.set_r(self.d, sign_extend32(mem.load_u8(address, Access::Nonsequential) as u32, 8)),
            (Opcode::STR, Length::Word, _) => mem.store_u32(address, value, Access::Nonsequential),
            (Opcode::STR, Length::Halfword, _) => mem.store_u16(address, value as u16, Access::Nonsequential),
            (Opcode::STR, Length::Byte, _) => mem.store_u8(address, value as u8, Access::Nonsequential),
        }
        if self.opcode == Opcode::LDR {
            mem.idle(1);
        }
    }

    pub fn disassemble(self, cond: Condition) -> String {
        format!(
            "{:?}{}{}{} R{}, {}",
            self.opcode,
            cond,
            if self.sign_extend { "S" } else { "" },
            match self.length {
                Length::Byte => "B",
                Length::Halfword => "H",
                Length::Word => "",
            },
            self.d,
            self.addressing_mode
        )
    }
}

impl Swap {
    #[inline(always)]
    pub fn execute(self, cpu: &mut CPU, mem: &mut Memory) {
        let address = cpu.get_r(self.n);
        let r_m = cpu.get_r(self.m);
        if self.byte {
            let old = mem.load_u8(address, Access::Nonsequential);
            mem.store_u8(address, r_m as u8, Access::Nonsequential);
            cpu.set_r(self.d, old as u32);
        } else {
            let old = mem.load_u32(address, Access::Nonsequential).rotate_right((address & 0b11) * 8);
            mem.store_u32(address, r_m, Access::Nonsequential);
            cpu.set_r(self.d, old);
        }
        mem.idle(1);
    }

    pub fn disassemble(self, cond: Condition) -> String {
        format!("SWP{}{} R{}, R{}, [R{}]", cond, if self.byte { "B" } else { "" }, self.d, self.m, self.n)
    }
}

impl AddressingMode {
    #[inline(always)]
    fn decode_arm(instruction: u32) -> AddressingMode {
        AddressingMode {
            n: get_bits32(instruction, 16, 4) as u8,
            u_is_add: get_bit(instruction, 23),
            offset: if get_bit(instruction, 25) {
                ShifterOperand::ShiftImmediate {
                    shift: Shift::decode(get_bits32(instruction, 5, 2)),
                    m: get_bits32(instruction, 0, 4) as u8,
                    shift_imm: get_bits32(instruction, 7, 5) as u8,
                }
            } else {
                ShifterOperand::Immediate {
                    immed: get_bits32(instruction, 0, 12) as u16,
                    rotate_imm: 0,
                }
            },
            indexing_mode: IndexingMode::decode_arm(instruction),
        }
    }

    #[inline(always)]
    fn decode_extra_arm(instruction: u32) -> AddressingMode {
        AddressingMode {
            n: get_bits32(instruction, 16, 4) as u8,
            u_is_add: get_bit(instruction, 23),
            offset: if get_bit(instruction, 22) {
                ShifterOperand::Immediate {
                    immed: (get_bits32(instruction, 8, 4) << 4 | get_bits32(instruction, 0, 4)) as u16,
                    rotate_imm: 0,
                }
            } else {
                ShifterOperand::Register {
                    m: get_bits32(instruction, 0, 4) as u8,
                }
            },
            indexing_mode: IndexingMode::decode_arm(instruction),
        }
    }

    #[inline(always)]
    fn execute(self, cpu: &mut CPU) -> u32 {
        let offset = self.offset.eval(cpu).0;
        let r_n = if self.n == REGISTER_PC { cpu.get_r(REGISTER_PC) & !0b11 } else { cpu.get_r(self.n) };
        let offset_address = if self.u_is_add { r_n.wrapping_add(offset) } else { r_n.wrapping_sub(offset) };
        match self.indexing_mode {
            IndexingMode::Offset => offset_address,
            IndexingMode::PreIndexed => {
                cpu.set_r(self.n, offset_address);
                offset_address
            }
            IndexingMode::PostIndexed => {
                cpu.set_r(self.n, offset_address);
                r_n
            }
        }
    }
}

impl IndexingMode {
    #[inline(always)]
    const fn decode_arm(instruction: u32) -> IndexingMode {
        match (get_bit(instruction, 24), get_bit(instruction, 21)) {
            (false, _) => IndexingMode::PostIndexed,
            (true, false) => IndexingMode::Offset,
            (true, true) => IndexingMode::PreIndexed,
        }
    }
}

impl Display for AddressingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sign = if self.u_is_add { "+" } else { "-" };
        let rhs = match self.offset {
            ShifterOperand::Immediate { immed, .. } => format!("#{}{:#X}", sign, immed),
            offset => format!("{}{}", sign, offset),
        };
        match self.indexing_mode {
            IndexingMode::Offset => write!(f, "[R{}, {}]", self.n, rhs),
            IndexingMode::PreIndexed => write!(f, "[R{}, {}]!", self.n, rhs),
            IndexingMode::PostIndexed => write!(f, "[R{}], {}", self.n, rhs),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strb() {
        let strb = Instruction::decode_arm(0xe5c33208);
        assert_eq!(format!("{}", strb.disassemble(Condition::EQ, 0)), "STREQB R3, [R3, #+0x208]");
    }

    #[test]
    fn test_ldrsd() {
        let instruction = Instruction::decode_arm(0xe17670f1);
        assert_eq!(format!("{}", instruction.disassemble(Condition::EQ, 0)), "LDREQSH R7, [R6, #-0x1]!");
    }

    #[test]
    fn test_strh_thumb() {
        let instruction = Instruction::decode_thumb(0x8021);
        assert_eq!(format!("{}", instruction.disassemble(Condition::AL, 0)), "STRH R1, [R4, #+0x0]");
    }

    #[test]
    fn test_ldr_scaled_register_post_indexed() {
        let instruction = Instruction::decode_arm(0xe6921103);
        assert_eq!(format!("{}", instruction.disassemble(Condition::AL, 0)), "LDR R1, [R2], +R3, LSL #0x2");
    }
}
