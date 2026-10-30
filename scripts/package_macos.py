#!/usr/bin/env python3
"""Build a relocatable macOS app and DMG using Cargo and Apple's local tools."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import tomllib
import zlib

ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"
NAME = "Photo Convr"
APP = DIST / f"{NAME}.app"
CONTENTS = APP / "Contents"
MACOS = CONTENTS / "MacOS"
FRAMEWORKS = CONTENTS / "Frameworks"
RESOURCES = CONTENTS / "Resources"


def run(*args, capture=False, env=None):
    result = subprocess.run([str(arg) for arg in args], cwd=ROOT, check=True,
                            text=True, stdout=subprocess.PIPE if capture else None, env=env)
    return result.stdout if capture else ""


def system_library(name):
    return name.startswith(("/usr/lib/", "/System/Library/"))


def dependencies(path):
    lines = run("otool", "-L", path, capture=True).splitlines()[1:]
    names = [line.strip().split(" (compatibility", 1)[0] for line in lines]
    # A dylib's own install name is also printed by otool -L.
    if path.suffix in (".dylib", ".so"):
        identity = run("otool", "-D", path, capture=True).splitlines()[1:]
        names = [name for name in names if name not in identity]
    return names


def rpaths(path):
    output = run("otool", "-l", path, capture=True)
    return re.findall(r"cmd LC_RPATH\n\s+cmdsize \d+\n\s+path (.*?) \(offset", output)


def resolve_dependency(name, owner, executable, inherited):
    def expand(value):
        return value.replace("@loader_path", str(owner.parent)).replace("@executable_path", str(executable.parent))
    if name.startswith("@rpath/"):
        for entry in [*rpaths(owner), *inherited]:
            candidate = Path(expand(entry)) / name.removeprefix("@rpath/")
            if candidate.is_file():
                return candidate.resolve()
    else:
        candidate = Path(expand(name))
        if candidate.is_absolute() and candidate.is_file():
            return candidate.resolve()
    raise RuntimeError(f"Cannot resolve {name} required by {owner}")


def version_tuple(version):
    return tuple(int(part) for part in version.split(".")) + (0,) * (3 - len(version.split(".")))


def minimum_macos(paths):
    versions = ["11.0"]
    for path in paths:
        output = run("otool", "-l", path, capture=True)
        versions += re.findall(r"\bminos (\d+(?:\.\d+)+)", output)
        versions += re.findall(r"LC_VERSION_MIN_MACOSX\n\s+cmdsize \d+\n\s+version (\d+(?:\.\d+)+)", output)
    return max(versions, key=version_tuple)


def copy_notices(source, destination):
    """Preserve upstream license texts and Homebrew source/version provenance."""
    destination.mkdir(parents=True, exist_ok=True)
    files = [p for p in source.iterdir() if p.is_file() and (
        p.name.lower().startswith(("license", "licence", "copying", "copyright", "notice"))
        or p.name in ("sbom.spdx.json", "INSTALL_RECEIPT.json"))]
    for path in files:
        shutil.copy2(path, destination / path.name)


class Bundler:
    def __init__(self, architecture):
        self.architecture = architecture
        self.copied = {}
        self.native = []

    def copy(self, source, destination, executable=None, inherited=()):
        source = source.resolve()
        if source in self.copied:
            return self.copied[source]
        if destination.exists():
            raise RuntimeError(f"Conflicting dependency filenames: {destination}")
        archs = run("lipo", "-archs", source, capture=True).split()
        if self.architecture not in archs:
            raise RuntimeError(f"{source} has {archs}, but this release needs {self.architecture}")
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
        destination.chmod(0o755)
        # Modifying load commands invalidates existing Homebrew signatures.
        subprocess.run(["codesign", "--remove-signature", str(destination)],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
        self.copied[source] = destination
        executable = executable or source
        search = [*rpaths(source), *inherited]
        for name in dependencies(source):
            if system_library(name):
                continue
            dependency = resolve_dependency(name, source, executable, search)
            bundled = self.copy(dependency, FRAMEWORKS / dependency.name, executable, search)
            relative = os.path.relpath(bundled, destination.parent)
            run("install_name_tool", "-change", name, f"@loader_path/{relative}", destination)
        if destination.suffix in (".dylib", ".so"):
            run("install_name_tool", "-id", f"@rpath/{destination.name}", destination)
        # Do not leave Homebrew search directories in the shipped binaries.
        for entry in rpaths(destination):
            if entry.startswith(("/opt/", "/usr/local/")):
                run("install_name_tool", "-delete_rpath", entry, destination)
        # Homebrew Cellar/<formula>/<version>/{bin,lib}/...
        ancestor = next((p for p in source.parents if (p / "INSTALL_RECEIPT.json").exists()), None)
        if ancestor:
            formula = ancestor.parent.name
            copy_notices(ancestor, RESOURCES / "ThirdParty" / "native" / formula)
        self.native.append(destination)
        return destination

    def verify_portability(self):
        for binary in self.native:
            for name in dependencies(binary):
                if system_library(name):
                    continue
                if not name.startswith("@loader_path/"):
                    raise RuntimeError(f"Nonportable dependency in {binary}: {name}")
                resolved = (binary.parent / name.removeprefix("@loader_path/")).resolve()
                if not resolved.is_relative_to(CONTENTS.resolve()) or not resolved.is_file():
                    raise RuntimeError(f"Dependency escapes app or is missing: {name}")


def png(size):
    """Original monochrome icon: opposing conversion arrows on a rounded tile."""
    def inside_polygon(x, y, points):
        inside = False
        j = len(points) - 1
        for i, (xi, yi) in enumerate(points):
            xj, yj = points[j]
            if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi) + xi:
                inside = not inside
            j = i
        return inside

    top = [(0.25, 0.30), (0.62, 0.30), (0.62, 0.21), (0.79, 0.38), (0.62, 0.55), (0.62, 0.45), (0.25, 0.45)]
    bottom = [(1 - x, 1 - y) for x, y in top]
    data = bytearray()
    # Supersampling keeps the small menu and Finder icons crisp.
    samples = 2 if size <= 256 else 1
    for row in range(size):
        data.append(0)
        for column in range(size):
            total_alpha = total_light = 0
            for dy in range(samples):
                for dx in range(samples):
                    x, y = (column + (dx + 0.5) / samples) / size, (row + (dy + 0.5) / samples) / size
                    cx, cy = min(max(x, 0.23), 0.77), min(max(y, 0.23), 0.77)
                    on_tile = (x - cx) ** 2 + (y - cy) ** 2 <= 0.17 ** 2
                    if on_tile:
                        total_alpha += 255
                        total_light += 255 if inside_polygon(x, y, top) or inside_polygon(x, y, bottom) else 18
            alpha = total_alpha // (samples * samples)
            light = total_light * 255 // total_alpha if total_alpha else 0
            data.extend((light, light, light, alpha))

    def chunk(kind, value):
        return struct.pack(">I", len(value)) + kind + value + struct.pack(">I", zlib.crc32(kind + value) & 0xffffffff)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(bytes(data), 9)) + chunk(b"IEND", b"")


def make_icon():
    chunks = []
    for size, kind in [(16, b"icp4"), (32, b"icp5"), (64, b"icp6"), (128, b"ic07"), (256, b"ic08"), (512, b"ic09"), (1024, b"ic10")]:
        data = png(size)
        chunks.append(kind + struct.pack(">I", len(data) + 8) + data)
    data = b"".join(chunks)
    (RESOURCES / "AppIcon.icns").write_bytes(b"icns" + struct.pack(">I", len(data) + 8) + data)


def rust_notices(target):
    metadata = json.loads(run("cargo", "metadata", "--locked", "--offline", "--format-version", "1", "--filter-platform", target, capture=True))
    packages = []
    for package in metadata["packages"]:
        if package["source"] is None:
            continue
        directory = Path(package["manifest_path"]).parent
        destination = RESOURCES / "ThirdParty" / "rust" / f'{package["name"]}-{package["version"]}'
        copy_notices(directory, destination)
        if package.get("license_file"):
            path = directory / package["license_file"]
            if path.is_file():
                shutil.copy2(path, destination / path.name)
        # Font licenses and notices are sometimes stored below the crate root.
        for path in directory.rglob("*"):
            if path.is_file() and path.name.lower().startswith(("license", "licence", "copying", "copyright", "notice", "ofl")):
                dest = destination / path.relative_to(directory)
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, dest)
        packages.append({key: package.get(key) for key in ("name", "version", "license", "repository", "source")})
    (RESOURCES / "ThirdParty" / "rust-dependencies.json").write_text(json.dumps(packages, indent=2) + "\n")


def smoke_test(helper):
    with tempfile.TemporaryDirectory(prefix="photo-convr-smoke-") as temporary:
        env = dict(os.environ, PATH="/usr/bin:/bin", LIBHEIF_PLUGIN_PATH=str(RESOURCES / "HeifPlugins"))
        run(helper, ROOT / "tests/fixtures/gradient.heic", Path(temporary) / "decoded.png", env=env)
        if not (Path(temporary) / "decoded.png").is_file():
            raise RuntimeError("Bundled HEIC decoder failed its smoke test")


def sign(path, identity):
    options = ["--force", "--sign", identity]
    if identity != "-":
        options += ["--timestamp", "--options", "runtime"]
    run("codesign", *options, path)


def build_app(identity):
    target = os.environ.get("TARGET") or re.search(r"^host: (.*)$", run("rustc", "-vV", capture=True), re.M)[1]
    arch = {"aarch64-apple-darwin": "arm64", "x86_64-apple-darwin": "x86_64"}.get(target)
    if not arch:
        raise RuntimeError("Only native macOS arm64 or x86_64 releases are supported")
    command = ["cargo", "build", "--release", "--locked"]
    if os.environ.get("TARGET"):
        command += ["--target", target]
    run(*command)
    metadata = json.loads(run("cargo", "metadata", "--no-deps", "--locked", "--format-version", "1", capture=True))
    executable = Path(metadata["target_directory"])
    if os.environ.get("TARGET"):
        executable /= target
    executable /= "release/photo-convr"
    tool = os.environ.get("HEIF_CONVERT") or shutil.which("heif-convert")
    if not tool:
        raise RuntimeError("Install libheif first: brew install libheif (or set HEIF_CONVERT)")
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    DIST.mkdir(exist_ok=True)
    if APP.exists():
        shutil.rmtree(APP)
    RESOURCES.mkdir(parents=True)
    (RESOURCES / "HeifPlugins").mkdir()
    bundler = Bundler(arch)
    bundler.copy(executable, MACOS / "photo-convr")
    bundler.copy(Path(tool), MACOS / "heif-convert")
    # Dynamic codec plugins are optional; Homebrew currently builds HEVC in.
    for plugin_dir in [Path(tool).resolve().parent.parent / "lib/libheif"]:
        if plugin_dir.is_dir():
            for plugin in sorted(plugin_dir.iterdir()):
                if plugin.suffix in (".so", ".dylib") and not plugin.is_symlink():
                    bundler.copy(plugin, RESOURCES / "HeifPlugins" / plugin.name)
    bundler.verify_portability()
    minimum = minimum_macos(bundler.native)
    plist = {
        "CFBundleName": NAME, "CFBundleDisplayName": NAME,
        "CFBundleIdentifier": os.environ.get("BUNDLE_ID", "com.photoconvr.desktop"),
        "CFBundleExecutable": "photo-convr", "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": version,
        "CFBundleVersion": os.environ.get("BUILD_NUMBER", version),
        "CFBundleIconFile": "AppIcon", "LSMinimumSystemVersion": minimum,
        "NSHighResolutionCapable": True, "LSApplicationCategoryType": "public.app-category.photography",
        "NSPrincipalClass": "NSApplication",
    }
    (CONTENTS / "Info.plist").write_bytes(plistlib.dumps(plist))
    (CONTENTS / "PkgInfo").write_bytes(b"APPL????")
    make_icon()
    rust_notices(target)
    manifest = {"version": version, "architecture": arch, "minimum_macos": minimum,
                "signing": "ad-hoc (local testing)" if identity == "-" else "Developer ID",
                "native_files": [str(p.relative_to(CONTENTS)) for p in bundler.native]}
    (RESOURCES / "build-info.json").write_text(json.dumps(manifest, indent=2) + "\n")
    for binary in bundler.native:
        if binary != MACOS / "photo-convr":
            sign(binary, identity)
    sign(APP, identity)
    run("codesign", "--verify", "--deep", "--strict", APP)
    smoke_test(MACOS / "heif-convert")
    print(f"\nBuilt {APP}\nArchitecture: {arch}; requires macOS {minimum} or later.", flush=True)
    return version, arch


def notarize(path, profile):
    run("xcrun", "notarytool", "submit", path, "--keychain-profile", profile, "--wait")
    run("xcrun", "stapler", "staple", path)
    run("xcrun", "stapler", "validate", path)


def make_dmg(version, architecture, identity, profile=None):
    # Stage under dist: never modify /Applications or require a mounted volume.
    with tempfile.TemporaryDirectory(prefix="dmg-stage-", dir=DIST) as temporary:
        stage = Path(temporary)
        shutil.copytree(APP, stage / APP.name, symlinks=True)
        (stage / "Applications").symlink_to("/Applications", target_is_directory=True)
        (stage / "Install.txt").write_text(f"Drag {NAME}.app into Applications, then open it.\n\nVersion {version} · {architecture}\nPhotos are converted locally. No Rust, Homebrew, or account is needed.\n")
        artifact = DIST / f"Photo-Convr-{version}-macOS-{architecture}.dmg"
        # Leave the prior DMG intact until creation/signing succeeds.
        pending = stage / "output.dmg"
        payload = stage / "payload"
        payload.mkdir()
        for path in list(stage.iterdir()):
            if path != payload:
                path.rename(payload / path.name)
        # makehybrid writes HFS+ directly to a file. Unlike `hdiutil create`,
        # it doesn't need a virtual disk device or mount permission.
        raw = stage / "uncompressed.dmg"
        run("hdiutil", "makehybrid", "-hfs", "-hfs-volume-name", NAME,
            "-o", raw, payload)
        run("hdiutil", "convert", raw, "-format", "UDZO", "-o", pending)
        if identity != "-":
            sign(pending, identity)
        if profile:
            notarize(pending, profile)
        run("hdiutil", "verify", pending)
        pending.replace(artifact)
    digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
    artifact.with_suffix(".dmg.sha256").write_text(f"{digest}  {artifact.name}\n")
    print(f"\nDMG: {artifact}\nSHA-256: {digest}", flush=True)
    if identity == "-":
        print("Local testing build: ad-hoc signed, not Apple-notarized.")
    return artifact


def main():
    if platform.system() != "Darwin":
        raise RuntimeError("DMG builds require macOS")
    mode = sys.argv[1] if len(sys.argv) == 2 else "help"
    if mode not in ("app", "dmg", "release"):
        raise RuntimeError("Usage: package_macos.py app|dmg|release")
    identity = os.environ.get("CODE_SIGN_IDENTITY", "-")
    profile = os.environ.get("NOTARY_PROFILE")
    if mode == "release" and (identity == "-" or not profile):
        raise RuntimeError("make release requires CODE_SIGN_IDENTITY and NOTARY_PROFILE; use make dmg for local testing")
    version, arch = build_app(identity)
    if mode == "release":
        archive = DIST / "notarization.zip"
        try:
            run("ditto", "-c", "-k", "--keepParent", APP, archive)
            # ZIPs cannot be stapled: submit the archive, staple the original app.
            run("xcrun", "notarytool", "submit", archive, "--keychain-profile", profile, "--wait")
            run("xcrun", "stapler", "staple", APP)
            run("xcrun", "stapler", "validate", APP)
        finally:
            archive.unlink(missing_ok=True)
    if mode != "app":
        make_dmg(version, arch, identity, profile if mode == "release" else None)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError, OSError) as error:
        print(f"Packaging failed: {error}", file=sys.stderr)
        sys.exit(1)
