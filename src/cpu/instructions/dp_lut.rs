use crate::cpu::CPU;
use lazy_static::lazy_static;
use std::sync::Mutex;

type InstructionFn = fn(&mut CPU, s: bool, n: u32, d: u32, so: u32, sco: bool);

const LUT_SIZE: usize = 1 << 4;

lazy_static! {
    static ref DP_LUT: Mutex<DataProcessingLut> = Mutex::new(DataProcessingLut::new());
}

pub struct DataProcessingLut {
    table: [InstructionFn; LUT_SIZE],
}

impl DataProcessingLut {
    fn new() -> Self {
        let mut lut = Self {
            table: [unknown_opcode_handler; LUT_SIZE],
        };
        lut.initialize();
        lut
    }

    fn initialize(&mut self) {
        self.table[0b1101] = mov; // MOV
    }

    pub(crate) fn get(opcode: u32) -> InstructionFn {
        let lut = DP_LUT.lock().unwrap();
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
