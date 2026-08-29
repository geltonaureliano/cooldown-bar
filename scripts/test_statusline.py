import importlib.util
import json
from pathlib import Path
import stat
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("hook", Path(__file__).with_name("claude-statusline.py"))
hook = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hook)

class StatuslineTests(unittest.TestCase):
    def test_zero_usage_and_context_are_not_dropped(self):
        data = {"model": {"display_name": "Claude"}, "context_window": {"used_percentage": 0}, "rate_limits": {"five_hour": {"used_percentage": 1}, "seven_day": {"used_percentage": 0}}}
        self.assertEqual(hook.status(data), "Claude  ctx 0%  5h 1%  7d 0%")

    def test_capture_is_private_atomic_and_contains_no_workspace_data(self):
        with tempfile.TemporaryDirectory() as folder:
            dest = Path(folder) / "claude.json"
            data = {"workspace": {"cwd": "private"}, "session_id": "private", "rate_limits": {"five_hour": {"used_percentage": 2, "resets_at": 1800000000, "private": "excluded"}}}
            hook.capture(data, dest)
            saved = json.loads(dest.read_text())
            self.assertEqual(saved, {"rate_limits": {"five_hour": {"used_percentage": 2, "resets_at": 1800000000}}})
            self.assertEqual(stat.S_IMODE(dest.stat().st_mode), 0o600)
            self.assertEqual(list(Path(folder).iterdir()), [dest])
            hook.capture({"rate_limits": None}, dest)
            self.assertEqual(json.loads(dest.read_text()), saved)

    def test_invalid_payload_cannot_replace_last_good_reading(self):
        with tempfile.TemporaryDirectory() as folder:
            dest = Path(folder) / "claude.json"
            hook.capture({"rate_limits": {"five_hour": {"used_percentage": 8}}}, dest)
            with self.assertRaises(ValueError):
                hook.capture({"rate_limits": {"five_hour": {"used_percentage": float("nan")}}}, dest)
            self.assertEqual(json.loads(dest.read_text())["rate_limits"]["five_hour"]["used_percentage"], 8)
            self.assertEqual(list(Path(folder).iterdir()), [dest])
