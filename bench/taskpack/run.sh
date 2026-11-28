#!/usr/bin/env bash
# Grade the offline Block 3 candidate task pack against hidden independent oracles.
#
#   bench/taskpack/run.sh matrix                # reference passes; every seed is caught
#   bench/taskpack/run.sh candidate <seed>      # grade one seed (expects a failure)
#   bench/taskpack/run.sh reference [family]    # grade a reference solution
#
# Plain `rustc`, no crates, no network, no model, no wallet provider. Generated workspaces live in
# `target/taskpack` and are never committed. Every expectation is pinned here rather than derived
# from the candidate, so a candidate cannot define its own success.
set -euo pipefail

TASKPACK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$TASKPACK_DIR/../.." && pwd)"
OUT_DIR="${TASKPACK_OUT_DIR:-$REPO_ROOT/target/taskpack}"

fail() {
  printf 'taskpack: %s\n' "$*" >&2
  exit 1
}

family_of() {
  case "$1" in
    *wallet*) printf 'wallet' ;;
    *transform*) printf 'transform' ;;
    *protocol*) printf 'protocol' ;;
    *) fail "unknown task family for seed $1" ;;
  esac
}

abi_for() {
  case "$1" in
    wallet) printf '%s\n' "$TASKPACK_DIR/abi/wallet_abi.rs" ;;
    transform) printf '%s\n' "$TASKPACK_DIR/abi/transform_abi.rs" ;;
    protocol) printf '%s\n' "$TASKPACK_DIR/abi/protocol_abi.rs" ;;
    *) fail "unknown task family $1" ;;
  esac
}

oracle_for() {
  case "$1" in
    wallet) printf '%s\n' "$TASKPACK_DIR/oracle/wallet_oracle.rs" ;;
    transform) printf '%s\n' "$TASKPACK_DIR/oracle/transform_oracle.rs" ;;
    protocol) printf '%s\n' "$TASKPACK_DIR/oracle/protocol_oracle.rs" ;;
    *) fail "unknown task family $1" ;;
  esac
}

reference_for() {
  case "$1" in
    wallet) printf '%s\n' "$TASKPACK_DIR/reference/wallet_ref.rs" ;;
    transform) printf '%s\n' "$TASKPACK_DIR/reference/transform_ref.rs" ;;
    protocol) printf '%s\n' "$TASKPACK_DIR/reference/protocol_ref.rs" ;;
    *) fail "unknown task family $1" ;;
  esac
}

# Every seed, the oracle signal it must fail, and one line of rationale. Transmutation seeds are
# caught by the corpus function they break, not by a single wallet check.
seeds() {
  cat <<'EOF'
01_wallet_overdraft|check:overdraft_is_rejected|wrapping subtraction turns an overdraft into a huge balance
02_wallet_exact_balance|check:exact_balance_is_allowed|an inclusive balance check rejects spending the whole balance
03_wallet_idempotent_replay|check:duplicate_retry_is_idempotent|the idempotency guard is skipped, so a retry applies twice
04_wallet_id_reuse|check:transfer_id_reuse_is_rejected|a known transfer ID is reported as a duplicate whatever the arguments
05_wallet_overflow_atomicity|check:overflow_is_atomic|the source is debited before the destination overflow is checked
06_wallet_invalid_requests|check:invalid_requests_are_rejected|the zero-amount guard is unreachable
07_wallet_tamper_detection|check:tamper_is_detected|chain validation never recomputes the event hash
08_transform_bit_reverse|function:bit_reverse|the reversal runs 63 steps instead of 64
09_transform_byte_swap|function:byte_swap|the lowest byte is never copied
10_transform_rotate_left|function:rotate_left|the rotation amount is reduced modulo 32 instead of 64
11_transform_rotate_right|function:rotate_right|the source index adds where it should subtract
12_transform_saturating_add|function:saturating_add|the addition wraps instead of clamping
13_transform_checked_mul|function:checked_mul|overflow is narrowed away instead of reported
14_transform_parity|function:parity|the parity predicate is inverted
15_protocol_double_begin|traces:begin_after_abort|`begin` is accepted without checking that a transaction already ended
16_protocol_commit_leaves_open|traces:empty_transaction,long_value,read_twice,sequence_zero_is_first,three_records,two_records|`commit` never clears `begun`, so every committed state is wrong
17_protocol_abort_keeps_records|traces:abort_discards|`abort` discards the transaction but keeps its records
18_protocol_sequence_unchecked|traces:duplicate_sequence,gap_sequence,sequence_gap_after_two|`put` appends whatever index it is handed
19_protocol_read_early|traces:read_before_commit|`read` is accepted as soon as a transaction begins
20_protocol_abort_without_transaction|traces:abort_before_begin|`abort` is accepted outside a transaction
EOF
}

row_for() {
  seeds | awk -F'|' -v seed="$1" '$1 == seed { print; found = 1 } END { if (!found) exit 1 }' ||
    fail "unknown seed: $1"
}

# Concatenate ABI + source + oracle into a single crate and compile it.
build() {
  local family="$1" source="$2" out="$3" label="$4"
  local work
  work="$(mktemp -d)"
  # assemble.sh owns the ABI-prepending and the structural checks on a candidate source.
  if ! bash "$TASKPACK_DIR/assemble.sh" "$source" "$work/assembled.rs"; then
    rm -rf "$work"
    fail "assemble.sh rejected $label"
  fi
  cat "$work/assembled.rs" >"$work/crate.rs"
  cat "$(oracle_for "$family")" >>"$work/crate.rs"
  if ! rustc --edition 2021 --crate-name taskpack_grader -D warnings -o "$out" "$work/crate.rs"; then
    rm -rf "$work"
    fail "compilation failed for $label"
  fi
  rm -rf "$work"
}

wallet_checks() {
  printf '%s\n' \
    constructor_rejections exact_balance_is_allowed overdraft_is_rejected \
    duplicate_retry_is_idempotent transfer_id_reuse_is_rejected overflow_is_atomic \
    invalid_requests_are_rejected tamper_is_detected
}

protocol_traces() {
  grep -v '^#' "$TASKPACK_DIR/corpus/protocol_traces.tsv" |
    grep -v '^[[:space:]]*$' |
    cut -f1 |
    sort -u
}

transform_functions() {
  printf '%s\n' \
    bit_reverse byte_swap rotate_left rotate_right saturating_add checked_mul parity
}

run_reference() {
  local family="${1:-wallet}"
  local binary output status
  [[ "$family" == "all" ]] && family=""
  mkdir -p "$OUT_DIR"
  if [[ -z "$family" || "$family" == "wallet" ]]; then
    binary="$OUT_DIR/reference-wallet"
    build wallet "$(reference_for wallet)" "$binary" "the wallet reference"
    status=0
    output="$("$binary")" || status=$?
    if [[ "$status" -ne 0 ]]; then
      printf '%s\n' "$output" >&2
      fail "the wallet reference solution failed the hidden oracle"
    fi
    printf '%s\n' "$output" | grep -qx 'oracle failed_checks=0 failed=' ||
      fail "unexpected wallet oracle summary for the reference solution"
    printf 'taskpack: wallet reference passes all %s checks\n' "$(wallet_checks | wc -l)"
  fi
  if [[ -z "$family" || "$family" == "protocol" ]]; then
    binary="$OUT_DIR/reference-protocol"
    build protocol "$(reference_for protocol)" "$binary" "the protocol reference"
    status=0
    output="$("$binary" "$TASKPACK_DIR/corpus/protocol_traces.tsv")" || status=$?
    if [[ "$status" -ne 0 ]]; then
      printf '%s\n' "$output" >&2
      fail "the protocol reference disagreed with the captured trace recording"
    fi
    printf '%s\n' "$output" | grep -qx 'oracle failed_traces=0 failed=' ||
      fail "unexpected protocol oracle summary for the reference solution"
    printf 'taskpack: protocol reference matches all %s captured traces\n' \
      "$(protocol_traces | wc -l)"
  fi
  if [[ -z "$family" || "$family" == "transform" ]]; then
    binary="$OUT_DIR/reference-transform"
    build transform "$(reference_for transform)" "$binary" "the transform reference"
    status=0
    output="$("$binary" "$TASKPACK_DIR/corpus/transmutation.tsv")" || status=$?
    if [[ "$status" -ne 0 ]]; then
      printf '%s\n' "$output" >&2
      fail "the transform reference solution disagreed with the differential corpus"
    fi
    printf '%s\n' "$output" | grep -qx 'grader failed_functions=0 failed=' ||
      fail "unexpected transform grader summary for the reference solution"
    printf 'taskpack: transform reference matches all %s corpus functions\n' \
      "$(transform_functions | wc -l)"
  fi
}

run_candidate() {
  local seed="$1"
  local row
  row="$(row_for "$seed")"
  local expectation="${row#*|}"
  expectation="${expectation%%|*}"
  local family
  family="$(family_of "$seed")"
  local source
  case "$family" in
    wallet) source="$TASKPACK_DIR/seeded/$seed/wallet.rs" ;;
    transform) source="$TASKPACK_DIR/seeded/$seed/func.rs" ;;
    protocol) source="$TASKPACK_DIR/seeded/$seed/protocol.rs" ;;
  esac
  [[ -f "$source" ]] || fail "missing seeded candidate $source"

  mkdir -p "$OUT_DIR"
  local binary="$OUT_DIR/$seed-grader"
  build "$family" "$source" "$binary" "seed $seed"

  local output=""
  local status=0
  case "$family" in
    transform) output="$("$binary" "$TASKPACK_DIR/corpus/transmutation.tsv")" || status=$? ;;
    protocol) output="$("$binary" "$TASKPACK_DIR/corpus/protocol_traces.tsv")" || status=$? ;;
    *) output="$("$binary")" || status=$? ;;
  esac
  [[ "$status" -ne 0 ]] || {
    printf '%s\n' "$output" >&2
    fail "seed $seed passed the oracle; the planted bug escaped"
  }

  local target="${expectation#*:}"
  case "${expectation%%:*}" in
    check)
      printf '%s\n' "$output" | grep -qx "check $target fail.*" || {
        printf '%s\n' "$output" >&2
        fail "seed $seed failed the wrong check; expected $target"
      }
      printf '%s\n' "$output" | grep -qx 'oracle failed_checks=1 failed=.*' || {
        printf '%s\n' "$output" >&2
        fail "seed $seed failed more checks than its single planted bug explains"
      }
      ;;
    function)
      printf '%s\n' "$output" | grep -qx "function $target rows=[0-9]* failures=[1-9][0-9]* .*" || {
        printf '%s\n' "$output" >&2
        fail "seed $seed did not fail the $target corpus rows"
      }
      printf '%s\n' "$output" | grep -qx "grader failed_functions=1 failed=$target" || {
        printf '%s\n' "$output" >&2
        fail "seed $seed broke a function other than $target"
      }
      ;;
    traces)
      # The pinned value is the exact set of traces the planted bug must break. It is written
      # here, never derived from the run, so a candidate cannot narrow its own failure set.
      local expected_failed sorted expected
      expected_failed="$target"
      sorted="$(printf '%s' "$expected_failed" | tr ',' '\n' | sort | paste -sd, -)"
      printf '%s\n' "$output" | grep -qx "oracle failed_traces=[0-9][0-9]* failed=$sorted" || {
        printf '%s\n' "$output" >&2
        fail "seed $seed did not fail exactly the pinned trace set: $sorted"
      }
      local trace
      for trace in $(printf '%s' "$expected_failed" | tr ',' ' '); do
        printf '%s\n' "$output" | grep -qx "trace $trace fail op=[-0-9]* .*" || {
          printf '%s\n' "$output" >&2
          fail "seed $seed did not report $trace as failing"
        }
      done
      ;;
  esac
  printf 'taskpack: seed %s caught (%s)\n' "$seed" "$target"
}

run_matrix() {
  run_reference all
  local row seed
  while IFS= read -r row; do
    seed="${row%%|*}"
    run_candidate "$seed"
  done < <(seeds)
  printf 'taskpack: matrix pass — 3 references and %s seeded candidates across %s wallet checks, %s corpus functions and %s captured traces\n' \
    "$(seeds | wc -l)" "$(wallet_checks | wc -l)" "$(transform_functions | wc -l)" \
    "$(protocol_traces | wc -l)"
}

main() {
  local mode="${1:-matrix}"
  case "$mode" in
    matrix) run_matrix ;;
    reference) run_reference "${2:-all}" ;;
    candidate)
      [[ $# -eq 2 ]] || fail "usage: run.sh candidate <seed>"
      run_candidate "$2"
      ;;
    *) fail "usage: run.sh [matrix|reference [wallet|transform|protocol|all]|candidate <seed>]" ;;
  esac
}

command -v rustc >/dev/null 2>&1 ||
  fail "rustc is required here; CI runs this matrix with a Rust toolchain"
main "$@"
