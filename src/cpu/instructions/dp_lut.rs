use crate::cpu::{instructions::set_nz_flags, CPU};

type InstructionFn = fn(&mut CPU, s: bool, n: u32, d: u32, so: u32, sco: bool);

const LUT_SIZE: usize = 1 << 4;

static mut DP_LUT: Option<DataProcessingLut> = None;

pub struct DataProcessingLut {
    table: [InstructionFn; LUT_SIZE],
}

impl DataProcessingLut {
    pub fn initialize() {
        let mut lut = Self {
            table: [unknown_opcode_handler; LUT_SIZE],
        };
        lut.setup_patterns();
        unsafe {
            DP_LUT = Some(lut);
        }
    }

    fn setup_patterns(&mut self) {
        self.table[0b1101] = mov; // MOV
    }

    pub(crate) fn get(opcode: u32) -> InstructionFn {
        unsafe {
            if let Some(ref lut) = DP_LUT {
                lut.table[opcode as usize]
            } else {
                panic!("Data Processing LUT not initialized!");
            }
        }
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
        set_nz_flags(cpu, cpu.r[d]);
        cpu.r.set_carry_flag(sco);
    }
}
