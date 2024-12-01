pub fn get_bits(data: u32, i: u32, len: u32) -> u32 {
    let mask = (1u32 << len) - 1;
    let shifted_mask = mask << i;
    (data & shifted_mask) >> i
}

pub fn set_bits(data: u32, i: u32, len: u32, value: u32) -> u32 {
    let mask = ((1u32 << len) - 1) << i;
    (data & !mask) | ((value << i) & mask)
}

pub fn get_bit(data: u32, i: u32) -> bool {
    let mask = 1 << i;
    (data & mask) > 0
}

pub fn set_bit(data: u32, i: u32, v: bool) -> u32 {
    let mask = 1 << i;
    if v {
        data | mask
    } else {
        data & !mask
    }
}

pub fn sign_extend(data: u32, data_len: u32) -> u32 {
    let shift = 32 - data_len;
    (((data << shift) as i32) >> shift) as u32
}

pub fn arithmetic_shift_right(data: u32, shift: u32) -> u32 {
    ((data as i32) >> shift) as u32
}

pub fn format_instruction(instruction: u32) -> String {
    format!(
        "Instruction: {:012b}\n\
        Bit Index:   27 26 25 24 23 22 21 20 07 06 05 04\n\
        Values:      {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2} {:<2}\n",
        instruction,
        get_bit(instruction, 27) as u32,
        get_bit(instruction, 26) as u32,
        get_bit(instruction, 25) as u32,
        get_bit(instruction, 24) as u32,
        get_bit(instruction, 23) as u32,
        get_bit(instruction, 22) as u32,
        get_bit(instruction, 21) as u32,
        get_bit(instruction, 20) as u32,
        get_bit(instruction, 7) as u32,
        get_bit(instruction, 6) as u32,
        get_bit(instruction, 5) as u32,
        get_bit(instruction, 4) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_bits() {
        assert_eq!(get_bits(0b00000000, 0, 8), 0);
        assert_eq!(get_bits(0b00000011, 0, 8), 3);
        assert_eq!(get_bits(0b00000011, 0, 1), 1);
        assert_eq!(get_bits(0b10110000, 6, 2), 2);
    }

    #[test]
    fn test_set_bits() {
        assert_eq!(set_bits(0b00000000, 0, 8, 0b11111111), 0b11111111);
        assert_eq!(set_bits(0b11111111, 0, 8, 0b00000000), 0b00000000);
        assert_eq!(set_bits(0b11111111, 4, 4, 0b0110), 0b01101111);
    }

    #[test]
    fn test_get_bit() {
        assert_eq!(get_bit(0b00000000, 0), false);
        assert_eq!(get_bit(0b00000001, 0), true);
    }

    #[test]
    fn test_set_bit() {
        assert_eq!(set_bit(0b00000000, 0, true), 0b00000001);
        assert_eq!(set_bit(0b00000001, 0, false), 0b00000000);
        assert_eq!(set_bit(0b00000001, 1, true), 0b00000011);
        assert_eq!(set_bit(0b00000011, 1, false), 0b00000001);
    }

    #[test]
    fn test_sign_extend() {
        // Test 12-bit sign extension (positive value)
        assert_eq!(sign_extend(0x7FF, 12), 0x7FF); // 2047, positive max
                                                   // Test 12-bit sign extension (negative value)
        assert_eq!(sign_extend(0x800, 12), 0xFFFFF800); // -2048 in signed terms

        // Test 8-bit sign extension (positive value)
        assert_eq!(sign_extend(0x7F, 8), 0x7F); // 127, positive max
                                                // Test 8-bit sign extension (negative value)
        assert_eq!(sign_extend(0x80, 8), 0xFFFFFF80); // -128 in signed terms

        // Test 16-bit sign extension (positive value)
        assert_eq!(sign_extend(0x7FFF, 16), 0x7FFF); // 32767, positive max
                                                     // Test 16-bit sign extension (negative value)
        assert_eq!(sign_extend(0x8000, 16), 0xFFFF8000); // -32768 in signed terms

        // Edge case: 1-bit sign extension
        assert_eq!(sign_extend(0x0, 1), 0x0); // 0
        assert_eq!(sign_extend(0x1, 1), 0xFFFFFFFF); // -1 in signed terms

        // Edge case: Full 32-bit value
        assert_eq!(sign_extend(0xFFFFFFFF, 32), 0xFFFFFFFF); // No change
        assert_eq!(sign_extend(0x7FFFFFFF, 32), 0x7FFFFFFF); // No change
    }

    #[test]
    fn test_arithmetic_shift_right() {
        // Test right shift on positive number
        assert_eq!(arithmetic_shift_right(0x7FFFFFFF, 1), 0x3FFFFFFF); // Divides by 2
        assert_eq!(arithmetic_shift_right(0x7FFFFFFF, 2), 0x1FFFFFFF); // Divides by 4

        // Test right shift on negative number
        assert_eq!(arithmetic_shift_right(0xFFFFFFFF, 1), 0xFFFFFFFF); // -1 stays -1
        assert_eq!(arithmetic_shift_right(0x80000000, 1), 0xC0000000); // -2147483648 >> 1
        assert_eq!(arithmetic_shift_right(0x80000001, 1), 0xC0000000); // Handles sign extension

        // Edge case: Shift by 0 (no change)
        assert_eq!(arithmetic_shift_right(0x12345678, 0), 0x12345678); // No shift
        assert_eq!(arithmetic_shift_right(0xFFFFFFFF, 0), 0xFFFFFFFF); // No shift
    }
}
