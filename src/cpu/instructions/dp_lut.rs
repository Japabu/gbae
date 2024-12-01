use crate::cpu::CPU;

type InstructionFn = fn(&mut CPU, s: bool, n: u32, d: u32, so: u32, sco: bool);

const LUT_SIZE: usize = 1 << 4;

pub struct DataProcessingLut {
    table: [InstructionFn; LUT_SIZE],
}

impl DataProcessingLut {
    pub fn new() -> Self {
        Self {
            table: [
                /*0000*/ unknown_opcode_handler,
                /*0001*/ unknown_opcode_handler,
                /*0010*/ unknown_opcode_handler,
                /*0011*/ unknown_opcode_handler,
                /*0100*/ unknown_opcode_handler,
                /*0101*/ unknown_opcode_handler,
                /*0110*/ unknown_opcode_handler,
                /*0111*/ unknown_opcode_handler,
                /*1000*/ unknown_opcode_handler,
                /*1001*/ unknown_opcode_handler,
                /*1010*/ unknown_opcode_handler,
                /*1011*/ unknown_opcode_handler,
                /*1100*/ unknown_opcode_handler,
                /*1101*/ mov,
                /*1110*/ unknown_opcode_handler,
                /*1111*/ unknown_opcode_handler,
            ],
        }
    }

    pub(crate) fn get(&self, opcode: u32) -> InstructionFn {
        self.table[opcode as usize]
    }
}

fn unknown_opcode_handler(_cpu: &mut CPU, s: bool, n: u32, d: u32, so: u32, sco: bool) {
    panic!(
        "Unknown data processing opcode: s={}, n={}, d={}, so={}, sco={}",
        s, n, d, so, sco
    );
}

fn mov(cpu: &mut CPU, s: bool, n: u32, d: u32, so: u32, sco: bool) {
    debug_assert_eq!(n, 0);

    cpu.r[d] = so;
    if s {
        cpu.set_flags(cpu.r[d]);
        cpu.r.set_carry_flag(sco);
    }
}
