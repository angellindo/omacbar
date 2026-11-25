"""Exclusive staging preserves a pre-existing temporary entry on EEXIST."""

from contextlib import ExitStack
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from rehearsal_fs import link_whole, write_json, write_whole


class RehearsalTempCollisions(unittest.TestCase):
    def test_existing_temp_file_is_neither_overwritten_nor_removed(self):
        pid = os.getpid()
        for operation, temp_name in (
            (lambda root, fd: write_json(root / "config", {"new": True}), f".config.tmp-{pid}"),
            (lambda root, fd: write_whole(fd, "config", b"new", 0o600), f".config.rehearsal-{pid}"),
            (lambda root, fd: link_whole(fd, "config", "other"), f".config.rehearsal-link-{pid}"),
        ):
            with self.subTest(temp_name=temp_name), ExitStack() as stack:
                root = Path(stack.enter_context(tempfile.TemporaryDirectory()))
                fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
                stack.callback(os.close, fd)
                config = root / "config"
                temp = root / temp_name
                config.write_bytes(b"original")
                temp.write_bytes(b"sentinel")
                before = {p: (p.stat().st_ino, p.read_bytes()) for p in (config, temp)}

                with self.assertRaises(FileExistsError):
                    operation(root, fd)

                self.assertEqual(set(root.iterdir()), {config, temp})
                self.assertEqual({p: (p.stat().st_ino, p.read_bytes()) for p in (config, temp)}, before)
