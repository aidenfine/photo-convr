from pathlib import Path
import tempfile
import unittest

from package_windows import stage


class WindowsPackagingTests(unittest.TestCase):
    def test_missing_decoder_fails_before_creating_package(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(RuntimeError, "Missing"):
                stage(root / "app.exe", root / "prefix", root / "output")
            self.assertFalse((root / "output").exists())

    def test_bundle_includes_runtime_plugins_and_notices(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            prefix = root / "prefix with spaces"
            files = ["bin/heif-dec.exe", "bin/libheif.dll", "bin/libde265.dll",
                     "lib/libheif/decoder.dll", "share/licenses/libheif/COPYING"]
            for name in files:
                path = prefix / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"fixture")
            app = root / "app.exe"
            app.write_bytes(b"app")
            output = root / "output"
            stage(app, prefix, output)
            for name in ["photo-convr.exe", "heif-dec.exe", "libheif.dll",
                         "libde265.dll", "HeifPlugins/decoder.dll",
                         "ThirdParty/libheif/COPYING"]:
                self.assertTrue((output / name).is_file(), name)
