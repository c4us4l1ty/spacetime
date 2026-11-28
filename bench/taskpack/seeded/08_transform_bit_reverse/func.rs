// SEEDED CANDIDATE — planted bug `bit-reverse-63-steps`.
//
// The reversal runs 63 steps instead of 64, so every result is missing the value's final bit
// and is off by a factor of two from the correct reversal.
//
// Do not "fix" this file before running the grader: the seeded state is the point. The
// candidate compiles against the pinned transform ABI, so the defect only shows up when the
// differential corpus is replayed.

pub fn bit_reverse(value: u64) -> u64 {
    // Planted bug: 63 steps instead of 64, so the last bit is never folded in.
    let mut result = 0_u64;
    let mut remaining = value;
    for _ in 0..63 {
        result = (result << 1) | (remaining & 1);
        remaining >>= 1;
    }
    result
}

pub fn byte_swap(value: u64) -> u64 {
    let mut result = 0_u64;
    for index in 0..8 {
        let byte = (value >> (index * 8)) & 0xff;
        result |= byte << ((7 - index) * 8);
    }
    result
}

pub fn rotate_left(value: u64, amount: u64) -> u64 {
    let shift = (amount % 64) as u32;
    let mut result = 0_u64;
    for destination in 0..64 {
        let source = (destination + 64 - shift) % 64;
        result |= ((value >> source) & 1) << destination;
    }
    result
}

pub fn rotate_right(value: u64, amount: u64) -> u64 {
    let shift = (amount % 64) as u32;
    let mut result = 0_u64;
    for destination in 0..64 {
        let source = (destination + shift) % 64;
        result |= ((value >> source) & 1) << destination;
    }
    result
}

pub fn saturating_add(left: u64, right: u64) -> u64 {
    (u128::from(left) + u128::from(right)).min(u128::from(u64::MAX)) as u64
}

pub fn checked_mul(left: u64, right: u64) -> Option<u64> {
    let product = u128::from(left) * u128::from(right);
    if product <= u128::from(u64::MAX) {
        Some(product as u64)
    } else {
        None
    }
}

pub fn parity(value: u64) -> bool {
    let mut remaining = value;
    let mut odd = false;
    while remaining != 0 {
        odd ^= remaining & 1 == 1;
        remaining >>= 1;
    }
    odd
}
