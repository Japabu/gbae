pub const fn get_bits32(data: u32, i: u8, len: u8) -> u32 {
    let mask = (1u32 << len) - 1;
    let shifted_mask = mask << i;
    (data & shifted_mask) >> i
}

pub const fn get_bits16(data: u16, i: u8, len: u8) -> u16 {
    let mask = (1u16 << len) - 1;
    let shifted_mask = mask << i;
    (data & shifted_mask) >> i
}

pub const fn set_bits32(data: u32, i: u8, len: u8, value: u32) -> u32 {
    let mask = ((1u32 << len) - 1) << i;
    (data & !mask) | ((value << i) & mask)
}

pub const fn get_bit(data: u32, i: u8) -> bool {
    let mask = 1 << i;
    (data & mask) != 0
}

pub const fn get_bit16(data: u16, i: u8) -> bool {
    let mask = 1 << i;
    (data & mask) != 0
}

pub const fn set_bit32(data: u32, i: u8, v: bool) -> u32 {
    let mask = 1 << i;
    if v {
        data | mask
    } else {
        data & !mask
    }
}

pub const fn sign_extend32(data: u32, data_len: u8) -> u32 {
    let shift = 32 - data_len;
    (((data << shift) as i32) >> shift) as u32
}

pub const fn arithmetic_shift_right(data: u32, shift: u8) -> u32 {
    ((data as i32) >> shift) as u32
}

pub const fn rotate_right_with_extend(c_flag: bool, data: u32) -> u32 {
    (c_flag as u32) << 31 | (data >> 1)
}

pub const fn add_with_flags(a: u32, b: u32) -> (u32, bool, bool) {
    let unsigned_result_64 = (a as u64).wrapping_add(b as u64);
    let signed_result_64 = (a as i32 as i64).wrapping_add(b as i32 as i64);
    let unsigned_overflow = unsigned_result_64 > u32::MAX as u64;
    let signed_overflow = signed_result_64 > i32::MAX as i64 || signed_result_64 < i32::MIN as i64;
    (unsigned_result_64 as u32, unsigned_overflow, signed_overflow)
}

pub const fn sub_with_flags(a: u32, b: u32) -> (u32, bool, bool) {
    let unsigned_result_64 = (a as u64).wrapping_sub(b as u64);
    let signed_result_64 = (a as i32 as i64).wrapping_sub(b as i32 as i64);
    let unsigned_underflow = unsigned_result_64 > u32::MAX as u64;
    let signed_overflow = signed_result_64 > i32::MAX as i64 || signed_result_64 < i32::MIN as i64;
    (unsigned_result_64 as u32, unsigned_underflow, signed_overflow)
}

pub fn add_with_flags_carry(a: u32, b: u32, carry: bool) -> (u32, bool, bool) {
    let unsigned_result_64 = (a as u64).wrapping_add(b as u64).wrapping_add(carry as u64);
    let signed_result_64 = (a as i32 as i64).wrapping_add(b as i32 as i64).wrapping_add(carry as i32 as i64);
    let unsigned_overflow = unsigned_result_64 > u32::MAX as u64;
    let signed_overflow = signed_result_64 > i32::MAX as i64 || signed_result_64 < i32::MIN as i64;
    (unsigned_result_64 as u32, unsigned_overflow, signed_overflow)
}

pub fn sub_with_flags_carry(a: u32, b: u32, carry: bool) -> (u32, bool, bool) {
    let unsigned_result_64 = (a as u64).wrapping_sub(b as u64).wrapping_sub(carry as u64);
    let signed_result_64 = (a as i32 as i64).wrapping_sub(b as i32 as i64).wrapping_sub(carry as i32 as i64);
    let unsigned_overflow = unsigned_result_64 > u32::MAX as u64;
    let signed_overflow = signed_result_64 > i32::MAX as i64 || signed_result_64 < i32::MIN as i64;
    (unsigned_result_64 as u32, unsigned_overflow, signed_overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_bits() {
        assert_eq!(get_bits32(0b00000000, 0, 8), 0);
        assert_eq!(get_bits32(0b00000011, 0, 8), 3);
        assert_eq!(get_bits32(0b00000011, 0, 1), 1);
        assert_eq!(get_bits32(0b10110000, 6, 2), 2);
    }

    #[test]
    fn test_set_bits() {
        assert_eq!(set_bits32(0b00000000, 0, 8, 0b11111111), 0b11111111);
        assert_eq!(set_bits32(0b11111111, 0, 8, 0b00000000), 0b00000000);
        assert_eq!(set_bits32(0b11111111, 4, 4, 0b0110), 0b01101111);
    }

    #[test]
    fn test_get_bit() {
        assert_eq!(get_bit(0b00000000, 0), false);
        assert_eq!(get_bit(0b00000001, 0), true);
    }

    #[test]
    fn test_set_bit() {
        assert_eq!(set_bit32(0b00000000, 0, true), 0b00000001);
        assert_eq!(set_bit32(0b00000001, 0, false), 0b00000000);
        assert_eq!(set_bit32(0b00000001, 1, true), 0b00000011);
        assert_eq!(set_bit32(0b00000011, 1, false), 0b00000001);
    }

    #[test]
    fn test_sign_extend() {
        // Test 12-bit sign extension (positive value)
        assert_eq!(sign_extend32(0x7FF, 12), 0x7FF); // 2047, positive max
                                                     // Test 12-bit sign extension (negative value)
        assert_eq!(sign_extend32(0x800, 12), 0xFFFFF800); // -2048 in signed terms

        // Test 8-bit sign extension (positive value)
        assert_eq!(sign_extend32(0x7F, 8), 0x7F); // 127, positive max
                                                  // Test 8-bit sign extension (negative value)
        assert_eq!(sign_extend32(0x80, 8), 0xFFFFFF80); // -128 in signed terms

        // Test 16-bit sign extension (positive value)
        assert_eq!(sign_extend32(0x7FFF, 16), 0x7FFF); // 32767, positive max
                                                       // Test 16-bit sign extension (negative value)
        assert_eq!(sign_extend32(0x8000, 16), 0xFFFF8000); // -32768 in signed terms

        // Edge case: 1-bit sign extension
        assert_eq!(sign_extend32(0x0, 1), 0x0); // 0
        assert_eq!(sign_extend32(0x1, 1), 0xFFFFFFFF); // -1 in signed terms

        // Edge case: Full 32-bit value
        assert_eq!(sign_extend32(0xFFFFFFFF, 32), 0xFFFFFFFF); // No change
        assert_eq!(sign_extend32(i32::MAX as u32, 32), i32::MAX as u32); // No change
    }

    #[test]
    fn test_arithmetic_shift_right() {
        // Test right shift on positive number
        assert_eq!(arithmetic_shift_right(i32::MAX as u32, 1), 0x3FFFFFFF); // Divides by 2
        assert_eq!(arithmetic_shift_right(i32::MAX as u32, 2), 0x1FFFFFFF); // Divides by 4

        // Test right shift on negative number
        assert_eq!(arithmetic_shift_right(0xFFFFFFFF, 1), 0xFFFFFFFF); // -1 stays -1
        assert_eq!(arithmetic_shift_right(i32::MIN as u32, 1), 0xC0000000); // -2147483648 >> 1
        assert_eq!(arithmetic_shift_right(0x80000001, 1), 0xC0000000); // Handles sign extension

        // Edge case: Shift by 0 (no change)
        assert_eq!(arithmetic_shift_right(0x12345678, 0), 0x12345678); // No shift
        assert_eq!(arithmetic_shift_right(0xFFFFFFFF, 0), 0xFFFFFFFF); // No shift
    }

    #[test]
    fn test_add_with_flags() {
        // Basic addition without carry or overflow
        assert_eq!(add_with_flags(1, 1), (2, false, false));

        // Test carry flag (unsigned overflow)
        assert_eq!(add_with_flags(u32::MAX, 1), (0, true, false));
        assert_eq!(add_with_flags(u32::MAX, 2), (1, true, false));

        // Test overflow flag (signed overflow)
        // Positive + Positive = Negative (overflow)
        assert_eq!(add_with_flags(i32::MAX as u32, 1), (i32::MIN as u32, false, true));

        // Negative + Negative = Positive (overflow)
        assert_eq!(add_with_flags(2u32.pow(31), 2u32.pow(31)), (0, true, true));

        // No overflow when adding numbers of different signs
        assert_eq!(add_with_flags(i32::MIN as u32, 1), (0x80000001, false, false));

        // Edge cases
        assert_eq!(add_with_flags(0, 0), (0, false, false));
    }

    #[test]
    fn test_sub_with_flags() {
        // Basic subtraction without borrow or overflow
        assert_eq!(sub_with_flags(2, 1), (1, false, false));

        // Test borrow flag (unsigned underflow)
        assert_eq!(sub_with_flags(0, 1), (0xFFFFFFFF, true, false));

        // Test overflow flag (signed overflow)
        // Positive - Negative = Negative (overflow)
        assert_eq!(sub_with_flags(i32::MAX as u32, i32::MIN as u32), (0xFFFFFFFF, true, true));

        // Negative - Positive = Positive (overflow)
        assert_eq!(sub_with_flags(i32::MIN as u32, 1), (i32::MAX as u32, false, true));

        // No overflow when subtracting numbers of same sign
        assert_eq!(sub_with_flags(i32::MIN as u32, i32::MIN as u32), (0, false, false));
        assert_eq!(sub_with_flags(1, 1), (0, false, false));

        // Edge cases
        assert_eq!(sub_with_flags(0, 0), (0, false, false));
        // i32::MIN as u32 - i32::MAX as u32 = 1 (overflow: negative - positive = positive)
        assert_eq!(sub_with_flags(i32::MIN as u32, i32::MAX as u32), (1, false, true));
    }

    #[test]
    fn test_add_with_flags_carry() {
        // Basic addition without carry or overflow
        assert_eq!(add_with_flags_carry(1, 1, false), (2, false, false));

        // Adding with carry input
        assert_eq!(add_with_flags_carry(1, 1, true), (3, false, false));

        // Test carry flag (unsigned overflow)
        assert_eq!(add_with_flags_carry(u32::MAX, 1, false), (0, true, false));
        assert_eq!(add_with_flags_carry(u32::MAX, 0, true), (0, true, false));

        // Test overflow flag (signed overflow)
        // Positive + Positive + Carry = Negative (overflow)
        assert_eq!(add_with_flags_carry(i32::MAX as u32, 1, true), (0x80000001, false, true));

        // Negative + Negative + Carry = Positive (overflow)
        assert_eq!(add_with_flags_carry(i32::MIN as u32, i32::MIN as u32, true), (1, true, true));

        // Edge cases
        assert_eq!(add_with_flags_carry(0, 0, false), (0, false, false));
        assert_eq!(add_with_flags_carry(0, 0, true), (1, false, false));

        // Test when adding numbers of different signs
        assert_eq!(add_with_flags_carry(i32::MIN as u32, 1, true), (0x80000002, false, false));

        // Adding with carry input and no overflow
        assert_eq!(add_with_flags_carry(0, 0, true), (1, false, false));

        // Unsigned overflow without signed overflow
        assert_eq!(add_with_flags_carry(u32::MAX, 0, true), (0, true, false));

        // Signed overflow without unsigned overflow
        assert_eq!(add_with_flags_carry(i32::MAX as u32, 1, false), (i32::MIN as u32, false, true));

        // Signed overflow with carry input
        assert_eq!(add_with_flags_carry(i32::MAX as u32, 1, true), (0x80000001, false, true));
    }

    #[test]
    fn test_sub_with_flags_carry() {
        // Basic subtraction without borrow or overflow
        assert_eq!(sub_with_flags_carry(2, 1, false), (1, false, false));

        // Subtraction with borrow input
        assert_eq!(sub_with_flags_carry(2, 1, true), (0, false, false));

        // Test borrow flag (unsigned underflow)
        assert_eq!(sub_with_flags_carry(0, 1, false), (0xFFFFFFFF, true, false));
        assert_eq!(sub_with_flags_carry(0, 0, true), (0xFFFFFFFF, true, false));

        // Test overflow flag (signed overflow)
        // Positive - Negative - Borrow = Negative (overflow)
        assert_eq!(sub_with_flags_carry(i32::MAX as u32, i32::MIN as u32, true), (0xFFFFFFFE, true, true));

        // Negative - Positive - Borrow = Positive (overflow)
        assert_eq!(sub_with_flags_carry(i32::MIN as u32, 1, true), (0x7FFFFFFE, false, true));

        // Edge cases
        assert_eq!(sub_with_flags_carry(0, 0, false), (0, false, false));
        assert_eq!(sub_with_flags_carry(0, 0, true), (0xFFFFFFFF, true, false));

        // Subtracting numbers of the same sign
        assert_eq!(sub_with_flags_carry(i32::MIN as u32, i32::MIN as u32, false), (0, false, false));
        assert_eq!(sub_with_flags_carry(1, 1, false), (0, false, false));

        // Subtracting with no borrow and no overflow
        assert_eq!(sub_with_flags_carry(1, 0, false), (1, false, false));

        // Subtracting with borrow input
        assert_eq!(sub_with_flags_carry(1, 0, true), (0, false, false));

        // Unsigned underflow without signed overflow
        assert_eq!(sub_with_flags_carry(0, 1, false), (u32::MAX, true, false));

        // Signed overflow without unsigned underflow
        assert_eq!(sub_with_flags_carry(i32::MIN as u32, 1, false), (i32::MAX as u32, false, true));

        // Subtracting large positive from large negative with borrow
        assert_eq!(sub_with_flags_carry(i32::MIN as u32, i32::MAX as u32, true), (0, false, true));
    }
}
