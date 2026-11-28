//! CANDIDATE EVENT ABI — pinned for the Block 3 integer-transmutation task family.
//!
//! `bench/taskpack/assemble.sh` prepends this file to every transmutation candidate, reference, and
//! oracle copy. Unlike the wallet family there is no in-process reference implementation here: the
//! independent oracle is the checked-in differential corpus at
//! `bench/taskpack/corpus/transmutation.tsv`, replayed by `oracle/transform_oracle.rs`. A candidate
//! is graded by exact agreement with that corpus, so "the candidate agrees with itself" can never
//! pass.
//!
//! ## Required candidate surface
//!
//! ```text
//! fn bit_reverse(x: u64) -> u64
//! fn byte_swap(x: u64) -> u64
//! fn rotate_left(x: u64, r: u64) -> u64
//! fn rotate_right(x: u64, r: u64) -> u64
//! fn saturating_add(a: u64, b: u64) -> u64
//! fn checked_mul(a: u64, b: u64) -> Option<u64>
//! fn parity(x: u64) -> bool
//! ```
//!
//! Rotation amounts are taken modulo 64, as `u64::rotate_left` documents.

/// One corpus row: `function`, two operands, and the expected result (`None` means the operation
/// must report overflow).
pub type CorpusRow = (String, u64, u64, Option<u64>);

/// Route a corpus function name to the candidate's implementation.
pub fn dispatch(function: &str, a: u64, b: u64) -> Result<u64, &'static str> {
    match function {
        "bit_reverse" => Ok(bit_reverse(a)),
        "byte_swap" => Ok(byte_swap(a)),
        "rotate_left" => Ok(rotate_left(a, b)),
        "rotate_right" => Ok(rotate_right(a, b)),
        "saturating_add" => Ok(saturating_add(a, b)),
        "checked_mul" => checked_mul(a, b).ok_or("overflow"),
        "parity" => Ok(u64::from(parity(a))),
        _ => Err("unknown function"),
    }
}

/// Parse one tab-separated corpus row. Blank lines and `#` comments are skipped by the caller.
pub fn parse_corpus_row(line: &str) -> Result<CorpusRow, String> {
    let mut fields = line.split('\t');
    let function = fields.next().ok_or("empty corpus row")?.trim().to_string();
    let a = parse_operand(fields.next(), "a")?;
    let b = parse_operand(fields.next(), "b")?;
    let expected_raw = fields.next().ok_or("missing expected field")?.trim();
    if fields.next().is_some() {
        return Err("trailing fields on a corpus row".to_string());
    }
    let expected = if expected_raw == "overflow" {
        None
    } else {
        Some(
            expected_raw
                .parse::<u64>()
                .map_err(|_| "expected field is not a u64 or 'overflow'".to_string())?,
        )
    };
    Ok((function, a, b, expected))
}

fn parse_operand(field: Option<&str>, name: &str) -> Result<u64, String> {
    field
        .ok_or_else(|| format!("missing operand {name}"))?
        .trim()
        .parse::<u64>()
        .map_err(|_| format!("operand {name} is not a u64"))
}
