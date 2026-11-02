#!/usr/bin/env python3
"""Build and verify an x64 Windows portable ZIP. Run on Windows with MSYS2."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
TARGET = "x86_64-pc-windows-msvc"


def stage(app, prefix, destination):
    """Use a dedicated MSYS2 prefix: all its runtime DLLs are redistributed."""
    helper = prefix / "bin/heif-dec.exe"
    if not helper.is_file():
        raise RuntimeError(f"Missing {helper}; install MSYS2's libheif package")
    licenses = prefix / "share/licenses"
    if not (licenses / "libheif").is_dir():
        raise RuntimeError("Missing libheif license notices")
    destination.mkdir(parents=True)
    shutil.copy2(app, destination / "photo-convr.exe")
    shutil.copy2(helper, destination)
    for dll in (prefix / "bin").glob("*.dll"):
        shutil.copy2(dll, destination)
    plugins = prefix / "lib/libheif"
    if plugins.is_dir():
        shutil.copytree(plugins, destination / "HeifPlugins")
    shutil.copytree(licenses, destination / "ThirdParty")
    shutil.copy2(ROOT / "README.md", destination)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--msys-prefix", type=Path, default=Path("C:/msys64/mingw64"))
    args = parser.parse_args()
    if os.name != "nt":
        parser.error("Build this package on Windows")
    subprocess.run(["cargo", "build", "--release", "--locked", "--target", TARGET],
                   cwd=ROOT, check=True)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    name = f"Photo-Convr-{version}-Windows-x64"
    dist = ROOT / "dist"
    dist.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="photo-convr-windows-") as directory:
        folder = Path(directory) / name
        stage(ROOT / f"target/{TARGET}/release/photo-convr.exe", args.msys_prefix, folder)
        # Test with MSYS2 removed from PATH, so missing bundled DLLs fail the build.
        env = os.environ.copy()
        env["PATH"] = str(Path(env["SystemRoot"]) / "System32")
        env["LIBHEIF_PLUGIN_PATH"] = str(folder / "HeifPlugins")
        output = Path(directory) / "decoded.png"
        subprocess.run([str(folder / "heif-dec.exe"),
                        str(ROOT / "tests/fixtures/gradient.heic"), str(output)],
                       env=env, cwd=directory, check=True, timeout=60)
        if not output.is_file() or output.read_bytes()[:8] != b"\x89PNG\r\n\x1a\n":
            raise RuntimeError("Bundled HEIC decoder did not produce a PNG")
        (folder / "README-Windows.txt").write_text(
            "Extract the entire ZIP, then double-click photo-convr.exe.\n"
            "Keep the DLLs, decoder and ThirdParty folder beside the app.\n"
            "This build is unsigned. No installation or administrator access is required.\n",
            encoding="utf-8")
        archive = Path(shutil.make_archive(str(Path(directory) / name), "zip", directory, name))
        final = dist / archive.name
        shutil.copy2(archive, final)
        digest = hashlib.sha256(final.read_bytes()).hexdigest()
        final.with_suffix(".zip.sha256").write_text(f"{digest}  {final.name}\n", encoding="ascii")
        print(final)


if __name__ == "__main__":
    main()
