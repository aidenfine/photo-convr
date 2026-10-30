import struct
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch

import package_macos as package


class PackagingTests(unittest.TestCase):
    def test_rpath_resolution_uses_original_loader_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            executable = root / 'prefix with spaces/bin/heif-convert'
            library = root / 'prefix with spaces/lib/libheif.1.dylib'
            library.parent.mkdir(parents=True)
            executable.parent.mkdir(parents=True)
            executable.touch()
            library.touch()
            with patch.object(package, 'rpaths', return_value=['@loader_path/../lib']):
                actual = package.resolve_dependency('@rpath/libheif.1.dylib', executable, executable, [])
            self.assertEqual(actual, library.resolve())

    def test_missing_dependencies_fail_instead_of_shipping_broken_bundle(self):
        with patch.object(package, 'rpaths', return_value=[]):
            with self.assertRaisesRegex(RuntimeError, 'Cannot resolve'):
                package.resolve_dependency('@rpath/missing.dylib', Path('/missing/helper'), Path('/missing/helper'), [])

    def test_minimum_os_comes_from_all_shipped_binaries(self):
        with patch.object(package, 'run', side_effect=[
            'cmd LC_BUILD_VERSION\n minos 11.0\n sdk 26.4',
            'cmd LC_BUILD_VERSION\n minos 26.0\n sdk 26.4',
        ]):
            self.assertEqual(package.minimum_macos([Path('app'), Path('library')]), '26.0')

    def test_absolute_homebrew_dependencies_are_rejected_by_audit(self):
        bundle = package.Bundler('arm64')
        bundle.native = [Path('/app/Contents/MacOS/heif-convert')]
        with patch.object(package, 'dependencies', return_value=['/opt/homebrew/lib/libheif.dylib']):
            with self.assertRaisesRegex(RuntimeError, 'Nonportable'):
                bundle.verify_portability()

    def test_icon_png_has_requested_dimensions(self):
        data = package.png(32)
        self.assertEqual(data[:8], b'\x89PNG\r\n\x1a\n')
        self.assertEqual(struct.unpack('>II', data[16:24]), (32, 32))


if __name__ == '__main__':
    unittest.main()
