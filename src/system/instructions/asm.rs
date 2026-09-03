use crate::system::cpu::{Register, INSTRUCTION_LEN_ARM, INSTRUCTION_LEN_THUMB};

use super::{
    branch::{Branch, BranchExchange, BranchLinkPrefix, BranchLinkSuffix},
    ctrl_ext::{Mrs, Msr, MsrOperand},
    data_processing::{DataProcessing, Opcode, Shift, ShifterOperand},
    load_store::{self, AddressingMode, Indexing, Length, LoadStore},
    load_store_multiple::{self, LoadStoreMultiple, RegisterList},
    multiply::{self, Multiply},
    swi::SoftwareInterrupt,
    Condition, Instruction,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Label(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fixup {
    Branch { offset: usize, thumb: bool, label: Label, link: bool, cond: Condition },
    Literal { offset: usize, thumb: bool, d: Register, value: u32 },
}

pub struct Assembler {
    base: u32,
    code: Vec<u8>,
    thumb: bool,
    labels: Vec<Option<u32>>,
    fixups: Vec<Fixup>,
}

impl Assembler {
    pub fn new(base: u32) -> Assembler {
        Assembler {
            base,
            code: Vec::new(),
            thumb: false,
            labels: Vec::new(),
            fixups: Vec::new(),
        }
    }

    pub fn arm(&mut self) -> &mut Assembler {
        self.thumb = false;
        self
    }

    pub fn thumb(&mut self) -> &mut Assembler {
        self.thumb = true;
        self
    }

    pub fn address(&self) -> u32 {
        self.base + self.code.len() as u32
    }

    pub fn label(&mut self) -> Label {
        self.labels.push(None);
        Label(self.labels.len() - 1)
    }

    pub fn place(&mut self, label: Label) -> &mut Assembler {
        self.labels[label.0] = Some(self.address());
        self
    }

    pub fn here(&mut self) -> Label {
        let label = self.label();
        self.place(label);
        label
    }

    pub fn word(&mut self, value: u32) -> &mut Assembler {
        self.code.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn halfword(&mut self, value: u16) -> &mut Assembler {
        self.code.extend_from_slice(&value.to_le_bytes());
        self
    }

    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Assembler {
        self.code.extend_from_slice(bytes);
        self
    }

    pub fn align(&mut self) -> &mut Assembler {
        while self.code.len() % 4 != 0 {
            self.code.push(0);
        }
        self
    }

    pub fn pad_to(&mut self, size: usize) -> &mut Assembler {
        assert!(self.code.len() <= size, "code is {} bytes, longer than {}", self.code.len(), size);
        self.code.resize(size, 0);
        self
    }

    pub fn emit(&mut self, instruction: Instruction) -> &mut Assembler {
        self.emit_if(Condition::AL, instruction)
    }

    pub fn emit_if(&mut self, cond: Condition, instruction: Instruction) -> &mut Assembler {
        if self.thumb {
            assert_eq!(cond, Condition::AL, "Thumb instructions are unconditional");
            let word = instruction.encode_thumb().unwrap_or_else(|| panic!("{:?} has no Thumb encoding", instruction));
            self.halfword(word)
        } else {
            let word = instruction.encode_arm(cond).unwrap_or_else(|| panic!("{:?} has no ARM encoding", instruction));
            self.word(word)
        }
    }

    pub fn b(&mut self, label: Label) -> &mut Assembler {
        self.branch(Condition::AL, label, false)
    }

    pub fn b_if(&mut self, cond: Condition, label: Label) -> &mut Assembler {
        self.branch(cond, label, false)
    }

    pub fn bl(&mut self, label: Label) -> &mut Assembler {
        self.branch(Condition::AL, label, true)
    }

    fn branch(&mut self, cond: Condition, label: Label, link: bool) -> &mut Assembler {
        self.fixups.push(Fixup::Branch {
            offset: self.code.len(),
            thumb: self.thumb,
            label,
            link,
            cond,
        });
        if self.thumb && link {
            self.word(0)
        } else if self.thumb {
            self.halfword(0)
        } else {
            self.word(0)
        }
    }

    pub fn ldr_literal(&mut self, d: Register, value: u32) -> &mut Assembler {
        self.fixups.push(Fixup::Literal {
            offset: self.code.len(),
            thumb: self.thumb,
            d,
            value,
        });
        if self.thumb {
            self.halfword(0)
        } else {
            self.word(0)
        }
    }

    pub fn pool(&mut self) -> &mut Assembler {
        self.align();
        let pending: Vec<Fixup> = self.fixups.iter().copied().filter(|fixup| matches!(fixup, Fixup::Literal { .. })).collect();
        self.fixups.retain(|fixup| !matches!(fixup, Fixup::Literal { .. }));
        let mut placed: Vec<(u32, u32)> = Vec::new();
        for fixup in pending {
            let Fixup::Literal { offset, thumb, d, value } = fixup else {
                continue;
            };
            let pool_address = match placed.iter().find(|(_, placed)| *placed == value) {
                Some((address, _)) => *address,
                None => {
                    let address = self.address();
                    self.word(value);
                    placed.push((address, value));
                    address
                }
            };
            let instruction_address = self.base + offset as u32;
            let pc = if thumb {
                (instruction_address + INSTRUCTION_LEN_THUMB * 2) & !0b11
            } else {
                instruction_address + INSTRUCTION_LEN_ARM * 2
            };
            let load = ldr(d, offset_address(Register::PC, pool_address - pc));
            self.patch(offset, thumb, Condition::AL, load);
        }
        self
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.pool();
        for fixup in std::mem::take(&mut self.fixups) {
            let Fixup::Branch { offset, thumb, label, link, cond } = fixup else {
                unreachable!("literal pools are flushed before branches are resolved");
            };
            let target = self.labels[label.0].expect("branch to a label that was never placed");
            let instruction_address = self.base + offset as u32;
            let distance = target.wrapping_sub(instruction_address);
            if thumb && link {
                let prefix = Instruction::BranchLinkPrefix(BranchLinkPrefix {
                    offset: distance.wrapping_sub(INSTRUCTION_LEN_THUMB * 2) & !0xFFF,
                });
                let suffix = Instruction::BranchLinkSuffix(BranchLinkSuffix {
                    offset: distance.wrapping_sub(INSTRUCTION_LEN_THUMB * 2) & 0xFFF,
                });
                self.patch(offset, true, cond, prefix);
                self.patch(offset + 2, true, cond, suffix);
            } else {
                let branch = Instruction::Branch(Branch {
                    link,
                    cond: if thumb { cond } else { Condition::AL },
                    offset: distance,
                });
                self.patch(offset, thumb, cond, branch);
            }
        }
        self.code
    }

    fn patch(&mut self, offset: usize, thumb: bool, cond: Condition, instruction: Instruction) {
        if thumb {
            let word = instruction.encode_thumb().unwrap_or_else(|| panic!("{:?} does not fit a Thumb encoding", instruction));
            self.code[offset..offset + 2].copy_from_slice(&word.to_le_bytes());
        } else {
            let word = instruction.encode_arm(cond).unwrap_or_else(|| panic!("{:?} does not fit an ARM encoding", instruction));
            self.code[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
        }
    }
}

pub mod registers {
    use crate::system::cpu::Register;

    pub const R0: Register = Register::R0;
    pub const R1: Register = Register::R1;
    pub const R2: Register = Register::R2;
    pub const R3: Register = Register::R3;
    pub const R4: Register = Register::R4;
    pub const R5: Register = Register::R5;
    pub const R6: Register = Register::R6;
    pub const R7: Register = Register::R7;
    pub const R8: Register = Register::R8;
    pub const R9: Register = Register::R9;
    pub const R10: Register = Register::R10;
    pub const R11: Register = Register::R11;
    pub const R12: Register = Register::R12;
    pub const SP: Register = Register::SP;
    pub const LR: Register = Register::LR;
    pub const PC: Register = Register::PC;
}

impl From<Register> for ShifterOperand {
    fn from(register: Register) -> ShifterOperand {
        ShifterOperand::Register(register)
    }
}

pub fn imm(value: u32) -> ShifterOperand {
    ShifterOperand::immediate(value)
}

pub fn lsl(m: Register, amount: u32) -> ShifterOperand {
    ShifterOperand::ShiftImmediate { shift: Shift::LSL, m, amount }
}

pub fn lsr(m: Register, amount: u32) -> ShifterOperand {
    ShifterOperand::ShiftImmediate { shift: Shift::LSR, m, amount }
}

pub fn asr(m: Register, amount: u32) -> ShifterOperand {
    ShifterOperand::ShiftImmediate { shift: Shift::ASR, m, amount }
}

pub fn ror(m: Register, amount: u32) -> ShifterOperand {
    ShifterOperand::ShiftImmediate { shift: Shift::ROR, m, amount }
}

pub fn lsl_by(m: Register, s: Register) -> ShifterOperand {
    ShifterOperand::ShiftRegister { shift: Shift::LSL, m, s }
}

pub fn lsr_by(m: Register, s: Register) -> ShifterOperand {
    ShifterOperand::ShiftRegister { shift: Shift::LSR, m, s }
}

pub fn asr_by(m: Register, s: Register) -> ShifterOperand {
    ShifterOperand::ShiftRegister { shift: Shift::ASR, m, s }
}

fn data_processing(opcode: Opcode, set_flags: bool, d: Register, n: Register, operand: impl Into<ShifterOperand>) -> Instruction {
    Instruction::DataProcessing(DataProcessing {
        opcode,
        set_flags,
        d,
        n,
        operand: operand.into(),
    })
}

macro_rules! three_operand {
    ($($name:ident $flags_name:ident $opcode:ident),*) => {
        $(
            pub fn $name(d: Register, n: Register, operand: impl Into<ShifterOperand>) -> Instruction {
                data_processing(Opcode::$opcode, false, d, n, operand)
            }

            pub fn $flags_name(d: Register, n: Register, operand: impl Into<ShifterOperand>) -> Instruction {
                data_processing(Opcode::$opcode, true, d, n, operand)
            }
        )*
    };
}

three_operand!(and ands AND, eor eors EOR, sub subs SUB, rsb rsbs RSB, add adds ADD, adc adcs ADC, sbc sbcs SBC, orr orrs ORR, bic bics BIC);

macro_rules! compare {
    ($($name:ident $opcode:ident),*) => {
        $(
            pub fn $name(n: Register, operand: impl Into<ShifterOperand>) -> Instruction {
                data_processing(Opcode::$opcode, true, n, n, operand)
            }
        )*
    };
}

compare!(tst TST, teq TEQ, cmp CMP, cmn CMN);

pub fn mov(d: Register, operand: impl Into<ShifterOperand>) -> Instruction {
    data_processing(Opcode::MOV, false, d, d, operand)
}

pub fn movs(d: Register, operand: impl Into<ShifterOperand>) -> Instruction {
    data_processing(Opcode::MOV, true, d, d, operand)
}

pub fn mvn(d: Register, operand: impl Into<ShifterOperand>) -> Instruction {
    data_processing(Opcode::MVN, false, d, d, operand)
}

fn multiply(set_flags: bool, d: Register, m: Register, s: Register) -> Instruction {
    Instruction::Multiply(Multiply {
        opcode: multiply::Opcode::MUL,
        set_flags,
        d,
        n: Register::R0,
        s,
        m,
    })
}

pub fn mul(d: Register, m: Register, s: Register) -> Instruction {
    multiply(false, d, m, s)
}

pub fn muls(d: Register, m: Register, s: Register) -> Instruction {
    multiply(true, d, m, s)
}

pub fn offset_address(n: Register, offset: u32) -> AddressingMode {
    let (add, offset) = if offset.wrapping_neg() < offset { (false, offset.wrapping_neg()) } else { (true, offset) };
    AddressingMode {
        n,
        add,
        offset: ShifterOperand::immediate(offset),
        indexing: Indexing::Offset,
    }
}

pub fn at(n: Register) -> AddressingMode {
    offset_address(n, 0)
}

pub fn post_increment(n: Register, offset: u32) -> AddressingMode {
    AddressingMode {
        indexing: Indexing::PostIndexed,
        ..offset_address(n, offset)
    }
}

pub fn pre_increment(n: Register, offset: u32) -> AddressingMode {
    AddressingMode {
        indexing: Indexing::PreIndexed,
        ..offset_address(n, offset)
    }
}

pub fn register_offset(n: Register, m: Register) -> AddressingMode {
    AddressingMode {
        n,
        add: true,
        offset: ShifterOperand::Register(m),
        indexing: Indexing::Offset,
    }
}

fn load_store(opcode: load_store::Opcode, length: Length, sign_extend: bool, d: Register, addressing: AddressingMode) -> Instruction {
    Instruction::LoadStore(LoadStore {
        opcode,
        length,
        sign_extend,
        d,
        addressing,
    })
}

macro_rules! load_store {
    ($($name:ident $opcode:ident $length:ident $sign_extend:literal),*) => {
        $(
            pub fn $name(d: Register, addressing: AddressingMode) -> Instruction {
                load_store(load_store::Opcode::$opcode, Length::$length, $sign_extend, d, addressing)
            }
        )*
    };
}

load_store!(ldr LDR Word false, str STR Word false, ldrh LDR Halfword false, strh STR Halfword false, ldrb LDR Byte false, strb STR Byte false, ldrsb LDR Byte true, ldrsh LDR Halfword true);

pub fn registers(list: impl IntoIterator<Item = Register>) -> RegisterList {
    list.into_iter().collect()
}

pub fn push(list: RegisterList) -> Instruction {
    Instruction::LoadStoreMultiple(LoadStoreMultiple {
        opcode: load_store_multiple::Opcode::STM,
        n: Register::SP,
        writeback: true,
        s: false,
        registers: list,
        addressing: load_store_multiple::AddressingMode::DecrementBefore,
    })
}

pub fn pop(list: RegisterList) -> Instruction {
    Instruction::LoadStoreMultiple(LoadStoreMultiple {
        opcode: load_store_multiple::Opcode::LDM,
        n: Register::SP,
        writeback: true,
        s: false,
        registers: list,
        addressing: load_store_multiple::AddressingMode::IncrementAfter,
    })
}

pub fn ldmia(n: Register, writeback: bool, list: RegisterList) -> Instruction {
    Instruction::LoadStoreMultiple(LoadStoreMultiple {
        opcode: load_store_multiple::Opcode::LDM,
        n,
        writeback,
        s: false,
        registers: list,
        addressing: load_store_multiple::AddressingMode::IncrementAfter,
    })
}

pub fn stmia(n: Register, writeback: bool, list: RegisterList) -> Instruction {
    Instruction::LoadStoreMultiple(LoadStoreMultiple {
        opcode: load_store_multiple::Opcode::STM,
        n,
        writeback,
        s: false,
        registers: list,
        addressing: load_store_multiple::AddressingMode::IncrementAfter,
    })
}

pub fn bx(m: Register) -> Instruction {
    Instruction::BranchExchange(BranchExchange { m })
}

pub fn swi(comment: u32) -> Instruction {
    Instruction::SoftwareInterrupt(SoftwareInterrupt { comment })
}

pub fn mrs_cpsr(d: Register) -> Instruction {
    Instruction::Mrs(Mrs { d, spsr: false })
}

pub fn msr_cpsr_control(operand: impl Into<ShifterOperand>) -> Instruction {
    let operand = match operand.into() {
        ShifterOperand::Immediate { value, .. } => MsrOperand::Immediate(value),
        ShifterOperand::Register(m) => MsrOperand::Register(m),
        operand => panic!("MSR takes an immediate or a register, not {:?}", operand),
    };
    Instruction::Msr(Msr { operand, fields: 0b0001, spsr: false })
}

pub fn subs_pc_lr(offset: u32) -> Instruction {
    data_processing(Opcode::SUB, true, Register::PC, Register::LR, imm(offset))
}

#[cfg(test)]
mod tests {
    use super::registers::*;
    use super::*;

    fn disassemble(bytes: &[u8], base: u32) -> Vec<String> {
        bytes
            .chunks_exact(4)
            .enumerate()
            .map(|(index, chunk)| {
                let word = u32::from_le_bytes(chunk.try_into().unwrap());
                Instruction::decode_arm(word).disassemble(Condition::decode_arm(word), base + index as u32 * 4)
            })
            .collect()
    }

    #[test]
    fn test_branches_and_literals_resolve() {
        let mut asm = Assembler::new(0x0800_0000);
        let start = asm.here();
        let target = asm.label();
        asm.ldr_literal(R0, 0x0400_0000)
            .b(target)
            .emit(mov(R1, imm(1)))
            .place(target)
            .emit_if(Condition::EQ, add(R2, R2, R1))
            .bl(start);
        let code = asm.finish();
        assert_eq!(
            disassemble(&code[..20], 0x0800_0000),
            ["LDR R0, [R15, #+0xC]", "B #0800000C", "MOV R1, #0x1", "ADDEQ R2, R2, R1", "BL #08000000"]
        );
        assert_eq!(&code[20..24], &0x0400_0000u32.to_le_bytes());
    }

    #[test]
    fn test_thumb_code() {
        let mut asm = Assembler::new(0x0300_0000);
        asm.thumb();
        let target = asm.label();
        asm.emit(movs(R0, imm(5)))
            .b_if(Condition::NE, target)
            .emit(adds(R0, R0, R1))
            .place(target)
            .emit(bx(LR))
            .ldr_literal(R1, 0x1234_5678);
        let code = asm.finish();
        let halfwords: Vec<u16> = code[..10].chunks_exact(2).map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap())).collect();
        assert_eq!(halfwords, [0x2005, 0xD100, 0x1840, 0x4770, 0x4900]);
        assert_eq!(&code[12..16], &0x1234_5678u32.to_le_bytes());
    }

    #[test]
    fn test_thumb_bl_pair() {
        let mut asm = Assembler::new(0x0800_0000);
        asm.thumb();
        let function = asm.label();
        asm.bl(function).emit(bx(LR)).place(function).emit(bx(LR));
        let code = asm.finish();
        assert_eq!(&code[..4], &[0x00, 0xF0, 0x01, 0xF8]);
    }

    #[test]
    fn test_addressing_helpers() {
        assert_eq!(ldr(R0, offset_address(R1, 0xFFFF_FFFC)).disassemble(Condition::AL, 0), "LDR R0, [R1, #-0x4]");
        assert_eq!(str(R0, post_increment(R1, 4)).disassemble(Condition::AL, 0), "STR R0, [R1], #+0x4");
        assert_eq!(ldrh(R0, pre_increment(R1, 2)).disassemble(Condition::AL, 0), "LDRH R0, [R1, #+0x2]!");
        assert_eq!(push(registers([R0, R1, LR])).disassemble(Condition::AL, 0), "STMDB R13!, {R0, R1, R14}");
        assert_eq!(subs_pc_lr(4).disassemble(Condition::AL, 0), "SUBS R15, R14, #0x4");
        assert_eq!(msr_cpsr_control(imm(0x12)).disassemble(Condition::AL, 0), "MSR CPSR_c, #0x12");
    }
}
