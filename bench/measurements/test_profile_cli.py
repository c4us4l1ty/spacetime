import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("profile_cli.py").resolve()
REPO_ROOT = SCRIPT.parents[2]
sys.path.insert(0, str(SCRIPT.parent))
import profile_cli


class ProfileCliTests(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory(prefix="spacetime-profile-test-")
        self.addCleanup(self.temp_dir.cleanup)
        self.root = Path(self.temp_dir.name)
        self.binary = self.root / "fake-spacetime-ctl"
        self.binary.write_text(
            "#!/usr/bin/env python3\n"
            "import time\n"
            "time.sleep(0.08)\n",
            encoding="utf-8",
        )
        self.binary.chmod(0o755)

    def run_profiler(self, *extra_args):
        return subprocess.run(
            [sys.executable, str(SCRIPT), *map(str, extra_args)],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            timeout=20,
            check=False,
        )

    def test_profiles_an_existing_headless_cli_workload_without_gate_authority(self):
        report_path = self.root / "profile.json"
        result = self.run_profiler(
            "--binary",
            self.binary,
            "--profile",
            "4gb_potato",
            "--host-label",
            "test-host",
            "--workload",
            "verify",
            "--runs",
            "2",
            "--sampling-ms",
            "10",
            "--out",
            report_path,
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(report_path.read_text(encoding="utf-8"))
        self.assertEqual(report["record_type"], "exploratory_headless_cli_profile")
        self.assertEqual(report["selected_target_profile"]["id"], "4gb_potato")
        self.assertEqual(report["workload"]["id"], "verify")
        self.assertTrue(report["summary"]["all_cli_runs_succeeded"])
        self.assertEqual(len(report["runs"]), 2)
        self.assertTrue(all(run["sample_count"] > 0 for run in report["runs"]))
        self.assertTrue(
            all(run["sampled_process_group_peak_rss_kb"] > 0 for run in report["runs"])
        )
        self.assertFalse(report["release_gate"]["gate_eligible"])
        self.assertEqual(report["release_gate"]["resource_gate"], "unmeasured_target_4gb")
        self.assertFalse(report["workload"]["surface_or_cell_launched"])
        self.assertNotIn("boot_ms", report["runs"][0])
        self.assertNotIn("tab_switch_ms", report["runs"][0])
        self.assertIn("physical provenance", report["release_gate"]["reason"])

    def test_failed_cli_workload_is_recorded_but_never_becomes_a_gate_pass(self):
        failing_binary = self.root / "failing-spacetime-ctl"
        failing_binary.write_text(
            "#!/usr/bin/env python3\n"
            "raise SystemExit(7)\n",
            encoding="utf-8",
        )
        failing_binary.chmod(0o755)
        report_path = self.root / "failed-profile.json"
        result = self.run_profiler(
            "--binary",
            failing_binary,
            "--profile",
            "4gb_potato",
            "--host-label",
            "test-host",
            "--workload",
            "verify",
            "--runs",
            "1",
            "--out",
            report_path,
        )

        self.assertEqual(result.returncode, 1)
        report = json.loads(report_path.read_text(encoding="utf-8"))
        self.assertEqual(report["runs"][0]["exit_code"], 7)
        self.assertFalse(report["summary"]["all_cli_runs_succeeded"])
        self.assertFalse(report["release_gate"]["gate_eligible"])

    def test_rejects_unregistered_profile(self):
        report_path = self.root / "profile.json"
        result = self.run_profiler(
            "--binary",
            self.binary,
            "--profile",
            "unknown-profile",
            "--host-label",
            "test-host",
            "--workload",
            "verify",
            "--runs",
            "1",
            "--out",
            report_path,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("unregistered target profile", result.stderr)
        self.assertFalse(report_path.exists())

    def test_rejects_profile_output_inside_repository(self):
        report_path = REPO_ROOT / "target" / "must-not-be-written.json"
        result = self.run_profiler(
            "--binary",
            self.binary,
            "--profile",
            "4gb_potato",
            "--host-label",
            "test-host",
            "--workload",
            "verify",
            "--runs",
            "1",
            "--out",
            report_path,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("outside the repository", result.stderr)
        self.assertFalse(report_path.exists())

    def test_rejects_existing_output_to_preserve_prior_evidence(self):
        report_path = self.root / "already-there.json"
        report_path.write_text("{}\n", encoding="utf-8")
        result = self.run_profiler(
            "--binary",
            self.binary,
            "--profile",
            "4gb_potato",
            "--host-label",
            "test-host",
            "--workload",
            "verify",
            "--runs",
            "1",
            "--out",
            report_path,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("already exists", result.stderr)
        self.assertEqual(report_path.read_text(encoding="utf-8"), "{}\n")

    def test_atomic_report_creation_never_replaces_a_concurrent_file(self):
        report_path = self.root / "concurrent-report.json"
        report_path.write_text("prior evidence\n", encoding="utf-8")

        with self.assertRaises(FileExistsError):
            profile_cli.write_json_new(report_path, {"new": "report"})

        self.assertEqual(report_path.read_text(encoding="utf-8"), "prior evidence\n")
        self.assertEqual(list(self.root.glob(".concurrent-report.json.*.tmp")), [])

    def test_rejects_invalid_runs_and_unsafe_host_label(self):
        for index, extra in enumerate((("--runs", "0"), ("--host-label", "bad label"))):
            with self.subTest(extra=extra):
                report_path = self.root / f"bad-{index}.json"
                args = [
                    "--binary",
                    str(self.binary),
                    "--profile",
                    "4gb_potato",
                    "--host-label",
                    "test-host",
                    "--workload",
                    "verify",
                    "--runs",
                    "1",
                    "--out",
                    str(report_path),
                ]
                if extra[0] == "--runs":
                    args[args.index("--runs") + 1] = extra[1]
                else:
                    args[args.index("--host-label") + 1] = extra[1]
                result = self.run_profiler(*args)
                self.assertEqual(result.returncode, 2)
                self.assertFalse(report_path.exists())


if __name__ == "__main__":
    unittest.main()
