#!/usr/bin/env bash
# Read-only host-fact collector for any recorded Spacetime target machine.
# No target class is inferred from detected RAM; an explicit, registered profile is required.
# This script does NOT produce a measurement file or a measured resource pass.
set -u
export LC_ALL=C

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd) || exit 1
profiles_file="$script_dir/../targets/profiles.tsv"

usage() {
  cat <<'EOF'
usage: capture-host-facts.sh --profile <registered-id> [--host-label <safe-label>]
       capture-host-facts.sh --list-profiles
       capture-host-facts.sh --help

The 4 GB potato profile is the release baseline, not the identity of every machine.
Select an explicit profile from bench/targets/profiles.tsv. GitHub Actions and other
CI runner facts are never target-machine evidence. Filled provenance stays out of band.
EOF
}

list_profiles() {
  printf '# id\tmemory_label\tmemory_budget_kb\tboot_budget_ms\ttab_switch_budget_ms\tcpu_budget_percent\trole\n'
  while IFS=$'\t' read -r id memory_label memory boot tab cpu role; do
    case "$id" in
      ''|\#*) continue ;;
    esac
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$id" "$memory_label" "$memory" "$boot" "$tab" "$cpu" "$role"
  done < "$profiles_file"
}

validate_token() {
  local value="$1"
  local label="$2"
  local limit="$3"
  if [ -z "$value" ] || [ "${#value}" -gt "$limit" ]; then
    printf '%s is empty or exceeds %s bytes\n' "$label" "$limit" >&2
    return 1
  fi
  case "$value" in
    --*)
      printf '%s must not look like a flag\n' "$label" >&2
      return 1
      ;;
    *[[:cntrl:]]*)
      printf '%s contains a control character\n' "$label" >&2
      return 1
      ;;
    *[!A-Za-z0-9._-]*)
      printf '%s contains unsupported characters\n' "$label" >&2
      return 1
      ;;
  esac
}

if [ ! -r "$profiles_file" ]; then
  printf 'target profile registry is unreadable: %s\n' "$profiles_file" >&2
  exit 2
fi

case "${1:-}" in
  --help)
    if [ "$#" -ne 1 ]; then
      printf -- '--help does not accept additional arguments\n' >&2
      exit 2
    fi
    usage
    exit 0
    ;;
  --list-profiles)
    if [ "$#" -ne 1 ]; then
      printf -- '--list-profiles does not accept additional arguments\n' >&2
      exit 2
    fi
    list_profiles
    exit 0
    ;;
esac

profile=""
host_label="unspecified"
saw_profile=0
saw_host_label=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --profile)
      if [ "$saw_profile" -ne 0 ]; then
        printf 'duplicate --profile flag\n' >&2
        exit 2
      fi
      if [ "$#" -lt 2 ]; then
        printf 'missing value for --profile\n' >&2
        exit 2
      fi
      profile="$2"
      validate_token "$profile" --profile 64 || exit 2
      saw_profile=1
      shift 2
      ;;
    --host-label)
      if [ "$saw_host_label" -ne 0 ]; then
        printf 'duplicate --host-label flag\n' >&2
        exit 2
      fi
      if [ "$#" -lt 2 ]; then
        printf 'missing value for --host-label\n' >&2
        exit 2
      fi
      host_label="$2"
      validate_token "$host_label" --host-label 128 || exit 2
      saw_host_label=1
      shift 2
      ;;
    *)
      printf 'unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [ "$saw_profile" -eq 0 ] || [ -z "$profile" ]; then
  printf 'explicit --profile is required; no target class is assumed\n' >&2
  usage >&2
  exit 2
fi

profile_row=""
baseline_id=""
while IFS=$'\t' read -r id memory_label memory boot tab cpu role; do
  case "$id" in
    ''|\#*) continue ;;
  esac
  if [ -z "$baseline_id" ] && [ "$role" = "baseline_gate" ]; then
    baseline_id="$id"
  fi
  if [ "$id" = "$profile" ]; then
    profile_row=$(printf '%s|%s|%s|%s|%s|%s|%s' \
      "$id" "$memory_label" "$memory" "$boot" "$tab" "$cpu" "$role")
  fi
done < "$profiles_file"

if [ -z "$baseline_id" ]; then
  printf 'target profile registry has no baseline_gate row\n' >&2
  exit 2
fi
if [ -z "$profile_row" ]; then
  printf 'unregistered --profile: %s\n' "$profile" >&2
  printf 'use --list-profiles to see registered profiles\n' >&2
  exit 2
fi

IFS='|' read -r p_id p_label p_memory p_boot p_tab p_cpu p_role <<< "$profile_row"

printf '# Spacetime target-machine host facts\n'
printf 'target_profile=%s\n' "$p_id"
printf 'profile_memory_label="%s"\n' "$p_label"
printf 'profile_memory_budget_kb=%s\n' "$p_memory"
printf 'profile_boot_budget_ms=%s\n' "$p_boot"
printf 'profile_tab_switch_budget_ms=%s\n' "$p_tab"
printf 'profile_cpu_budget_percent=%s\n' "$p_cpu"
printf 'profile_role=%s\n' "$p_role"
printf 'baseline_gate_profile=%s\n' "$baseline_id"
printf 'host_label=%s\n' "$host_label"
printf 'purpose=out_of_band_host_evidence_only\n'
printf 'profile_is_not_a_resource_claim=true\n'
printf 'captured_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || printf unavailable)"
printf 'cwd=%s\n' "$PWD"

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

section 'identity'
printf 'hostname=%s\n' "$(hostname 2>/dev/null || printf unavailable)"
run_if_available uname uname -a
printf 'kernel_name=%s\n' "$(uname -s 2>/dev/null || printf unavailable)"
printf 'kernel_release=%s\n' "$(uname -r 2>/dev/null || printf unavailable)"
printf 'machine_arch=%s\n' "$(uname -m 2>/dev/null || printf unavailable)"
show_file /etc/os-release

section 'exact hardware model (serial numbers intentionally omitted)'
for identity_file in \
  /sys/class/dmi/id/sys_vendor \
  /sys/class/dmi/id/product_name \
  /sys/class/dmi/id/product_version; do
  if [ -r "$identity_file" ]; then
    printf '%s=' "${identity_file##*/}"
    tr '\n' ' ' < "$identity_file"
    printf '\n'
  else
    printf 'unavailable: %s\n' "$identity_file"
  fi
done
if [ -r /sys/firmware/devicetree/base/model ]; then
  printf 'device_tree_model='
  tr -d '\000' < /sys/firmware/devicetree/base/model
  printf '\n'
fi

section 'cpu'
grep -m 1 -E '^(model name|Hardware|Processor)[[:space:]]*:' /proc/cpuinfo 2>/dev/null || printf 'cpu_model=unavailable\n'
run_if_available nproc nproc
if command -v lscpu >/dev/null 2>&1; then
  lscpu | grep -E '^(Architecture|CPU\(s\)|On-line CPU\(s\) list|Thread\(s\) per core|Core\(s\) per socket|Socket\(s\)|Model name):' || true
else
  printf 'unavailable: lscpu\n'
fi

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

section 'storage and filesystem'
run_if_available findmnt findmnt --target "$PWD" -no SOURCE,FSTYPE,OPTIONS
run_if_available df df -hT "$PWD"
if command -v lsblk >/dev/null 2>&1; then
  lsblk -o NAME,TYPE,SIZE,ROTA,TRAN,MOUNTPOINTS 2>/dev/null || true
else
  printf 'unavailable: lsblk\n'
fi
run_if_available stat stat -f -c 'filesystem_at_cwd=%T' .
run_if_available findmnt findmnt -no FSTYPE,PROPAGATION /
printf 'copy_on_write_or_reflink=requires_filesystem_specific_review\n'

section 'virtualization and cgroups'
if [ -e /dev/kvm ]; then
  printf 'kvm_device=present\n'
else
  printf 'kvm_device=absent_or_unavailable\n'
fi
run_if_available stat stat -f -c 'cgroup_filesystem=%T' /sys/fs/cgroup
if [ -r /sys/fs/cgroup/cgroup.controllers ]; then
  printf 'cgroup_version=2\n'
  printf 'cgroup_controllers='
  tr '\n' ' ' < /sys/fs/cgroup/cgroup.controllers
  printf '\n'
else
  printf 'cgroup_version=1_or_unavailable\n'
fi
if [ -r /sys/module/zswap/parameters/enabled ]; then
  printf 'zswap_enabled='
  tr '\n' ' ' < /sys/module/zswap/parameters/enabled
  printf '\n'
else
  printf 'zswap_enabled=unavailable\n'
fi

section 'boundary'
printf '%s\n' 'host_facts_are_not_performance_measurements'
printf '%s\n' 'profile_selection_is_explicit_not_inferred_from_detected_ram'
printf '%s\n' 'github_actions_or_other_ci_runner_facts_are_not_target_machine_evidence'
printf '%s\n' 'record_this_output_out_of_band_with_PROVENANCE-TEMPLATE.md'
printf '%s\n' 'release_baseline_gate_remains_unmeasured_until_physical_provenance_is_accepted'
