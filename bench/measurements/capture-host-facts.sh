#!/usr/bin/env bash
# Read-only host-fact collector for the recorded 4 GB "potato" machine.
# It does NOT produce a measurement file or a measured resource pass.
# Run it on the target machine and keep its output with the out-of-band provenance record.
set -u
export LC_ALL=C

if [ "$#" -ne 0 ]; then
  printf 'usage: %s (no arguments)\n' "$0" >&2
  printf 'This script emits host evidence only; it does not write a measurement file.\n' >&2
  exit 2
fi

section() {
  printf '\n## %s\n' "$1"
}

show_file() {
  local path="$1"
  if [ -r "$path" ]; then
    sed -n '1,40p' "$path"
  else
    printf 'unavailable: %s\n' "$path"
  fi
}

run_if_available() {
  local tool="$1"
  shift
  if command -v "$tool" >/dev/null 2>&1; then
    "$@"
  else
    printf 'unavailable: %s\n' "$tool"
  fi
}

printf '# Spacetime target-machine host facts\n'
printf 'purpose=out_of_band_host_evidence_only\n'
printf 'captured_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || printf unavailable)"
printf 'cwd=%s\n' "$PWD"

section 'identity'
printf 'hostname=%s\n' "$(hostname 2>/dev/null || printf unavailable)"
run_if_available uname uname -a
printf 'kernel_name=%s\n' "$(uname -s 2>/dev/null || printf unavailable)"
printf 'kernel_release=%s\n' "$(uname -r 2>/dev/null || printf unavailable)"
printf 'machine_arch=%s\n' "$(uname -m 2>/dev/null || printf unavailable)"
show_file /etc/os-release

section 'cpu'
grep -m 1 -E '^(model name|Hardware|Processor)[[:space:]]*:' /proc/cpuinfo 2>/dev/null || printf 'cpu_model=unavailable\n'
run_if_available nproc nproc

section 'memory and swap'
grep -E '^(MemTotal|MemAvailable|SwapTotal|SwapFree):' /proc/meminfo 2>/dev/null || printf 'memory_facts=unavailable\n'
show_file /proc/swaps
run_if_available swapon swapon --show
if compgen -G '/sys/block/zram*' >/dev/null; then
  printf 'zram_devices:\n'
  ls -1 /sys/block/zram* 2>/dev/null || true
else
  printf 'zram_devices=none_or_unavailable\n'
fi

section 'filesystem and virtualization'
run_if_available stat stat -f -c 'filesystem_at_cwd=%T' .
run_if_available findmnt findmnt -no FSTYPE,PROPAGATION /
if [ -e /dev/kvm ]; then
  printf 'kvm_device=present\n'
else
  printf 'kvm_device=absent_or_unavailable\n'
fi
run_if_available stat stat -f -c 'cgroup_filesystem=%T' /sys/fs/cgroup

section 'boundary'
printf '%s\n' 'host_facts_are_not_performance_measurements'
printf '%s\n' 'github_actions_or_other_ci_runner_facts_are_not_target_machine_evidence'
printf '%s\n' 'record_this_output_out_of_band_with_PROVENANCE-TEMPLATE.md'
printf '%s\n' 'resource_gate_remains_unmeasured_until_physical_provenance_is_accepted'
