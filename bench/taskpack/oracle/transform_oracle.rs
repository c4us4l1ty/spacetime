//! Hidden differential grader for the Block 3 integer-transmutation tasks.
//!
//! The expectation is never derived from the candidate. It comes exclusively from
//! `bench/taskpack/corpus/transmutation.tsv`, whose 2,498 expected values were computed with
//! Python's arbitrary-precision integers. `run.sh` passes the corpus path as `argv[1]` so the
//! concatenated crate needs no `include_str!`.
//!
//! Output per function, then a summary:
//!
//! ```text
//! function <name> rows=<n> failures=<m> first=<detail or ->
//! grader failed_functions=<n> failed=<comma-separated names>
//! ```

use std::collections::BTreeMap;

fn main() {
    let path = match std::env::args().nth(1) {
        Some(path) => path,
        None => {
            eprintln!("usage: grader <corpus.tsv>");
            std::process::exit(2);
        }
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("cannot read corpus {path}: {error}");
            std::process::exit(2);
        }
    };

    let mut total: BTreeMap<String, u64> = BTreeMap::new();
    let mut failures: BTreeMap<String, (u64, String)> = BTreeMap::new();
    let mut parsed = 0u64;
    for (index, line) in text.lines().enumerate() {
        let line = line.trim_end();
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let (function, a, b, expected) = match parse_corpus_row(line) {
            Ok(row) => row,
            Err(error) => {
                eprintln!("corpus row {}: {error}", index + 1);
                std::process::exit(2);
            }
        };
        parsed += 1;
        *total.entry(function.clone()).or_insert(0) += 1;
        let actual = dispatch(&function, a, b).ok();
        if actual != expected {
            let entry = failures
                .entry(function.clone())
                .or_insert_with(|| (0, String::new()));
            entry.0 += 1;
            if entry.1.is_empty() {
                entry.1 = format!(
                    "{function}({a}, {b}) returned {actual:?}, corpus expects {expected:?}"
                );
            }
        }
    }
    if parsed == 0 {
        eprintln!("the corpus contained no rows");
        std::process::exit(2);
    }

    let mut failed_names: Vec<String> = Vec::new();
    for (function, rows) in &total {
        match failures.get(function) {
            Some((count, first)) => {
                println!("function {function} rows={rows} failures={count} first={first}");
                failed_names.push(function.clone());
            }
            None => println!("function {function} rows={rows} failures=0 first=-"),
        }
    }
    println!(
        "grader failed_functions={} failed={}",
        failed_names.len(),
        failed_names.join(",")
    );
    if !failed_names.is_empty() {
        std::process::exit(1);
    }
}
