"""Algorithmic tests for local workflow bookkeeping (no engine runs)."""

import contextlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import workflow


class WorkflowTests(unittest.TestCase):
    def test_parse_failures_and_stage_timings(self) -> None:
        text = "\x1b[31m FAIL [ 1.25s] (3/4) hanabi-search example\x1b[0m\n"
        text += "FAIL [ 1.25s] (3/4) hanabi-search example\n"
        text += "PASS [ 0.01s] (4/4) hanabi-search passing\n"
        text += "CHECK TIMING: Rust tests: 278s (exit 100)\n"
        failures, stages = workflow.parse_log(text)
        self.assertEqual(failures, ["hanabi-search example"])
        self.assertEqual(stages, {"Rust tests": {"seconds": 278, "exit_code": 100}})

    def test_failure_delta_does_not_hide_existing_failures(self) -> None:
        self.assertEqual(workflow.failure_delta(["new", "old"], ["old", "gone"]),
                         {"new": ["new"], "still_failing": ["old"], "no_longer_reported": ["gone"]})

    def test_overlapping_command_intervals_are_not_double_counted(self) -> None:
        self.assertEqual(workflow.union_seconds([(1, 5), (3, 7), (8, 9)]), 7)
        self.assertEqual(workflow.union_seconds([]), 0)

    def test_command_preserves_failure_and_records_output(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(workflow, "STORE", Path(directory)):
                with contextlib.redirect_stdout(io.StringIO()):
                    code = workflow.run_command("focused", [sys.executable, "-c",
                                                "print('evidence'); raise SystemExit(7)"])
                self.assertEqual(code, 7)
                records = list(Path(directory).glob("*.json"))
                self.assertEqual(len(records), 1)
                record = workflow.read_json(records[0])
                self.assertEqual(record["exit_code"], 7)
                self.assertIn("evidence", Path(record["log"]).read_text())
                self.assertGreater(record["seconds"], 0)

    def test_task_lifecycle_and_explicit_wait(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with patch.object(workflow, "STORE", Path(directory)):
                for args in (["start", "example"], ["wait-start"], ["wait-end"]):
                    with patch.object(sys, "argv", ["workflow", *args]):
                        self.assertEqual(workflow.main(), 0)
                with patch.object(sys, "argv", ["workflow", "start", "overwrite"]):
                    with self.assertRaises(ValueError):
                        workflow.main()
                output = io.StringIO()
                with patch.object(sys, "argv", ["workflow", "finish"]), contextlib.redirect_stdout(output):
                    self.assertEqual(workflow.main(), 0)
                summary = json.loads(output.getvalue())
                self.assertGreaterEqual(summary["recorded_wait_seconds"], 0)
                self.assertEqual(summary["full_runs"], 0)
                self.assertFalse((Path(directory) / "active.json").exists())

    def test_fast_log_cannot_be_a_full_baseline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "fast.log"
            log.write_text("CHECK TIMING: Final ownership: 0s (exit 0)\n")
            with patch.object(sys, "argv", ["workflow", "baseline", str(log), "--revision", "example"]):
                with self.assertRaises(ValueError):
                    workflow.main()

    def test_check_help_and_unknown_option_do_not_run_checks(self) -> None:
        script = workflow.ROOT / "scripts" / "check.sh"
        help_result = subprocess.run(["bash", str(script), "--help"], capture_output=True, text=True, check=False)
        self.assertEqual(help_result.returncode, 0)
        self.assertIn("--fast", help_result.stdout)
        bad = subprocess.run(["bash", str(script), "--typo"], capture_output=True, text=True, check=False)
        self.assertEqual(bad.returncode, 2)


if __name__ == "__main__":
    unittest.main()
