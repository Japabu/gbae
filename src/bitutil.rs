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

/// Adds two 32-bit unsigned integers and returns the result along with carry and overflow flags.
///
/// # Arguments
///
/// * `op1` - The first operand.
/// * `op2` - The second operand.
///
/// # Returns
///
/// A tuple containing:
/// * The 32-bit result of the addition.
/// * A boolean value indicating whether a carry occurred.
/// * A boolean value indicating whether an overflow occurred.
pub fn add_with_flags(op1: u32, op2: u32) -> (u32, bool, bool) {
    let (result, carry) = op1.overflowing_add(op2);
    let sign_op1 = get_bit(op1, 31);
    let sign_op2 = get_bit(op2, 31);
    let sign_result = get_bit(result, 31);
    let overflow = sign_op1 == sign_op2 && sign_op1 != sign_result;
    (result, carry, overflow)
}

pub fn sub_with_flags(a: u32, b: u32) -> (u32, bool, bool) {
    let (result, borrow) = a.overflowing_sub(b);
    let sign_a = get_bit(a, 31);
    let sign_b = get_bit(b, 31);
    let sign_result = get_bit(result, 31);
    let overflow = sign_a != sign_b && sign_a != sign_result;
    (result, borrow, overflow)
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

    #[test]
    fn test_add_with_flags() {
        // Basic addition without carry or overflow
        assert_eq!(add_with_flags(1, 1), (2, false, false));

        // Test carry flag (unsigned overflow)
        assert_eq!(add_with_flags(0xFFFFFFFF, 1), (0, true, false));
        assert_eq!(add_with_flags(0xFFFFFFFF, 2), (1, true, false));

        // Test overflow flag (signed overflow)
        // Positive + Positive = Negative (overflow)
        assert_eq!(add_with_flags(0x7FFFFFFF, 1), (0x80000000, false, true));

        // Negative + Negative = Positive (overflow)
        assert_eq!(add_with_flags(0x80000000, 0x80000000), (0, true, true));

        // No overflow when adding numbers of different signs
        assert_eq!(add_with_flags(0x80000000, 1), (0x80000001, false, false));

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
        assert_eq!(sub_with_flags(0x7FFFFFFF, 0x80000000), (0xFFFFFFFF, true, true));

        // Negative - Positive = Positive (overflow)
        assert_eq!(sub_with_flags(0x80000000, 1), (0x7FFFFFFF, false, true));

        // No overflow when subtracting numbers of same sign
        assert_eq!(sub_with_flags(0x80000000, 0x80000000), (0, false, false));
        assert_eq!(sub_with_flags(1, 1), (0, false, false));

        // Edge cases
        assert_eq!(sub_with_flags(0, 0), (0, false, false));
        // 0x80000000 - 0x7FFFFFFF = 1 (overflow: negative - positive = positive)
        assert_eq!(sub_with_flags(0x80000000, 0x7FFFFFFF), (1, false, true));
    }
}
