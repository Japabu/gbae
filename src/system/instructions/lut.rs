use crate::system::instructions::{branch, ctrl_ext, dp, format_instruction, ls, lsm};
use crate::{bitutil::get_bits, system::cpu::CPU};

macro_rules! dp_handler {
    ($dp_handler:expr) => {
        |cpu: &mut CPU, instruction: u32| dp::handler(cpu, instruction, $dp_handler)
    };
}

macro_rules! add_dp_patterns {
    ($lut:expr, $($opcode:expr => $handler:expr),* $(,)?) => {
        $(
            $lut.add_pattern(&format!("001{}x xxxx", $opcode), dp_handler!($handler), dp::dec);
            $lut.add_pattern(&format!("000{}x xxx0", $opcode), dp_handler!($handler), dp::dec);
            $lut.add_pattern(&format!("000{}x 0xx1", $opcode), dp_handler!($handler), dp::dec);
        )*
    };
}

type InstructionHandlerFn = fn(&mut CPU, instruction: u32);
type DecoderFn = fn(u32) -> String;

const LUT_SIZE: usize = 1 << 12;

static mut INSTRUCTION_LUT: Option<InstructionLut> = None;

pub struct InstructionLut {
    handlers: [InstructionHandlerFn; LUT_SIZE],
    decoders: [DecoderFn; LUT_SIZE],
}

impl InstructionLut {
    pub fn initialize() {
        let mut lut = Self {
            handlers: [Self::unknown_instruction_handler; LUT_SIZE],
            decoders: [Self::unknown_instruction_decoder; LUT_SIZE],
        };
        lut.setup_patterns();
        unsafe {
            INSTRUCTION_LUT = Some(lut);
        }
    }

    pub fn get_handler(instruction: u32) -> InstructionHandlerFn {
        unsafe {
            if let Some(ref lut) = INSTRUCTION_LUT {
                lut.handlers[Self::index(instruction)]
            } else {
                panic!("Instruction LUT not initialized!");
            }
        }
    }

    pub fn get_decoder(instruction: u32) -> String {
        unsafe {
            if let Some(ref lut) = INSTRUCTION_LUT {
                (lut.decoders[Self::index(instruction)])(instruction)
            } else {
                panic!("Instruction LUT not initialized!");
            }
        }
    }

    fn index(instruction: u32) -> usize {
        // Bits 4-7 and 20-27 can be used to differentiate instructions and then index into the table
        let upper = get_bits(instruction, 20, 8);
        let lower = get_bits(instruction, 4, 4);
        ((upper << 4) | lower) as usize
    }

    fn setup_patterns(&mut self) {
        // data processing
        add_dp_patterns!(
            self,
            "0000" => dp::and,
            "0001" => dp::eor,
            "0010" => dp::sub,
            "0011" => dp::rsb,
            "0100" => dp::add,
            "0101" => dp::adc,
            "0110" => dp::sbc,
            "0111" => dp::rsc,
            "1000" => dp::tst,
            "1001" => dp::teq,
            "1010" => dp::cmp,
            "1011" => dp::cmn,
            "1100" => dp::orr,
            "1101" => dp::mov,
            "1110" => dp::bic,
            "1111" => dp::mvn,
        );
        self.add_pattern("101xxxxx xxxx", branch::b, branch::b_dec);
        // extensions
        self.add_pattern("00010x10 0000", ctrl_ext::msr_reg, ctrl_ext::msr_reg_dec);
        self.add_pattern("00010010 0001", branch::bx, branch::bx_dec);
        // load store
        self.add_pattern("010xxxxx xxxx", ls::handler, ls::dec);
        self.add_pattern("011xxxxx xxx0", ls::handler, ls::dec);
        // load store multiple
        self.add_pattern("100xxxxx xxxx", lsm::handler, lsm::dec);
    }

    fn add_pattern(&mut self, pattern: &str, handler: InstructionHandlerFn, decoder: DecoderFn) {
        let pattern = pattern.to_string().replace(" ", "");
        assert_eq!(pattern.len(), 12, "Pattern must be 12 bits long");

        // Determine which bits are fixed and which are wildcards
        let mut base_index = 0usize;
        let mut wildcard_positions = Vec::new();

        for (i, c) in pattern.chars().enumerate() {
            match c {
                '0' => {}
                '1' => base_index |= 1 << (11 - i),
                'x' => wildcard_positions.push(11 - i),
                _ => panic!("Invalid character in pattern: {}", c),
            }
        }

        // Generate all possible combinations of the wildcard bits
        let num_wildcards = wildcard_positions.len();
        let num_combinations = 1 << num_wildcards;

        for i in 0..num_combinations {
            let mut index = base_index;
            for (j, &pos) in wildcard_positions.iter().enumerate() {
                if (i & (1 << j)) != 0 {
                    index |= 1 << pos;
                } else {
                    index &= !(1 << pos);
                }
            }
            self.handlers[index] = handler;
            self.decoders[index] = decoder;
        }
    }

    fn unknown_instruction_handler(cpu: &mut CPU, instruction: u32) {
        cpu.print_registers();
        cpu.print_status();
        panic!(
            "Encountered an unknown instruction: {}",
            format_instruction(instruction)
        );
    }

    fn unknown_instruction_decoder(_instruction: u32) -> String {
        "???".to_string()
    }
}
