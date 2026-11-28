#!/usr/bin/env bash
# Assemble one self-contained candidate workspace.
#
# The pinned ABI is prepended to the candidate so the resulting file is standalone plain `rustc`.
# Seeded and reference sources are checked in *without* the ABI, and CI re-assembles them, so a
# seeded candidate can never quietly contain its own private copy of the contract.
#
# Two candidate layouts are recognised:
#
#   seeded/<seed>/wallet.rs   wallet family; must define exactly one `pub mod wallet {`
#   seeded/<seed>/func.rs     transmutation family; must define the pinned free functions
#
#   bench/taskpack/assemble.sh seeded/01_wallet_overdraft/wallet.rs target/taskpack/01.rs
#
# Usage in Rust tests: `assemble.sh <source-under-taskpack> <destination>`.
set -euo pipefail

TASKPACK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

[[ $# -eq 2 ]] || {
  printf 'usage: assemble.sh <source.rs> <destination.rs>\n' >&2
  exit 2
}
source_path="$1"
destination_path="$2"

# Accept both an absolute path and a path relative to the task pack.
if [[ ! -f "$source_path" ]]; then
  source_path="$TASKPACK_DIR/$source_path"
fi

[[ -f "$source_path" ]] || {
  printf 'assemble: missing candidate source %s\n' "$source_path" >&2
  exit 1
}

case "$source_path" in
  "$TASKPACK_DIR"/seeded/*/wallet.rs) family=wallet ;;
  "$TASKPACK_DIR"/reference/wallet_ref.rs) family=wallet ;;
  "$TASKPACK_DIR"/seeded/*/func.rs) family=transform ;;
  "$TASKPACK_DIR"/reference/transform_ref.rs) family=transform ;;
  *)
    printf 'assemble: %s is not a seeded candidate or the reference solution\n' "$source_path" >&2
    exit 1
    ;;
esac

family_abi="wallet_abi"
[[ "$family" == "transform" ]] && family_abi="transform_abi"
if grep -q '^pub mod wallet {' "$TASKPACK_DIR/abi/$family_abi.rs"; then
  printf 'assemble: the shared ABI must not define pub mod wallet\n' >&2
  exit 1
fi

if grep -q 'include!' "$source_path"; then
  printf "assemble: %s must be self-contained and not use include!\n" "$source_path" >&2
  exit 1
fi

if grep -q 'CANDIDATE EVENT ABI' "$source_path"; then
  printf 'assemble: %s already contains the ABI; check it in without the ABI prefix\n' "$source_path" >&2
  exit 1
fi

if [[ "$family" == "wallet" ]]; then
  if ! grep -qx 'pub mod wallet {' "$source_path"; then
    printf "assemble: %s must define exactly one 'pub mod wallet {'\n" "$source_path" >&2
    exit 1
  fi
  if [[ "$(grep -cx 'pub mod wallet {' "$source_path")" -ne 1 ]]; then
    printf "assemble: %s defines 'pub mod wallet {' more than once\n" "$source_path" >&2
    exit 1
  fi
else
  for function in bit_reverse byte_swap rotate_left rotate_right saturating_add checked_mul parity; do
    if ! grep -q "^pub fn $function(" "$source_path"; then
      printf 'assemble: %s is missing the pinned function %s\n' "$source_path" "$function" >&2
      exit 1
    fi
  done
  if grep -q '^pub mod wallet {' "$source_path"; then
    printf 'assemble: %s is a transmutation candidate and must not define a wallet module\n' "$source_path" >&2
    exit 1
  fi
fi

mkdir -p "$(dirname "$destination_path")"
{
  cat "$TASKPACK_DIR/abi/$family_abi.rs"
  printf '\n'
  cat "$source_path"
} >"$destination_path"

printf 'assemble: %s + ABI -> %s\n' "${source_path#"$TASKPACK_DIR/"}" "$destination_path" >&2
