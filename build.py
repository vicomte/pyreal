#!/usr/bin/env python3
"""Build Pyreal.app and sign it with a personal Apple certificate.

    python3 build.py                check, build, sign and verify the app
    python3 build.py --check        check only: tools, certificate, git
    python3 build.py --allow-dirty  build uncommitted changes too

Settings live in build.ini beside this file. The packaging itself is `cargo xtask package pyreal`;
this script supplies its environment, unpacks Pyreal.app into dist/<commit>/ and checks it. The app
carries the base art pack base-pack.toml pins, which `cargo xtask base-pack build` makes in
target/base-pack/ first; the player chooses any other in the client.
"""

import argparse
import configparser
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent
CONFIG = ROOT / "build.ini"

# Folders cargo and the packager write into, kept out of Dropbox sync.
BUILD_DIRS = ["target"]
# Where the packager writes its archives, before they are unpacked into out_dir.
STAGE = "target/build-py-package"
# The base art pack the app carries, as `cargo xtask base-pack build` writes it.
BASE_PACK = "target/base-pack/base-pack.dat"
# Where the icon with build.ini's own emblem is rendered, for the packager to take.
ICON_STAGE = "target/build-py-icon"
# Dropbox on macOS File Provider skips anything carrying this attribute.
DROPBOX_IGNORE = "com.apple.fileprovider.ignore#P"
# The bundle's identifier, which the check reads back.
BUNDLE_ID = "com.jacobjohnston.Pyreal"
# Notarisation and imported certificates are for a release workflow; a local build does neither.
CLEARED_ENV = [
    "APPLE_ID",
    "APPLE_PASSWORD",
    "APPLE_TEAM_ID",
    "APPLE_API_KEY",
    "APPLE_API_ISSUER",
    "APPLE_API_KEY_PATH",
    "APPLE_CERTIFICATE",
    "APPLE_CERTIFICATE_PASSWORD",
]


class Failed(Exception):
    pass


def resolve(path):
    p = Path(os.path.expanduser(path.strip()))
    return p if p.is_absolute() else ROOT / p


def load_config():
    if not CONFIG.is_file():
        raise Failed(f"no {CONFIG.name} beside {Path(__file__).name}")
    cfg = configparser.ConfigParser()
    cfg.read(CONFIG)
    try:
        return {
            "identity": cfg["signing"]["identity"].strip(),
            "out_dir": resolve(cfg["build"]["out_dir"]),
            "path_prepend": [
                str(resolve(p)) for p in cfg["build"].get("path_prepend", "").split(",") if p.strip()
            ],
            "applications_dir": cfg["install"].get("applications_dir", "").strip(),
            "icon_emblem": (
                resolve(cfg["build"]["icon_emblem"]) if cfg["build"].get("icon_emblem", "").strip() else None
            ),
        }
    except KeyError as e:
        raise Failed(f"{CONFIG.name} is missing {e}") from None


def build_env(cfg):
    env = dict(os.environ)
    for name in CLEARED_ENV:
        env.pop(name, None)
    env["PATH"] = os.pathsep.join(cfg["path_prepend"] + [env.get("PATH", "")])
    env["APPLE_SIGNING_IDENTITY"] = cfg["identity"]
    return env


def run(args, env, capture=True):
    """Run `args` in the project folder; its output, or None when it fails or is missing."""
    try:
        out = subprocess.run(
            args, cwd=ROOT, env=env, text=True, capture_output=capture, check=False
        )
    except FileNotFoundError:
        return None
    if out.returncode != 0:
        return None
    return out.stdout if capture else ""


def version_tuple(text):
    m = re.search(r"(\d+)\.(\d+)", text or "")
    return (int(m.group(1)), int(m.group(2))) if m else None


def check(cfg, env, allow_dirty):
    """Every problem found, as lines to show; empty when the build can start."""
    problems = []

    needed = version_tuple(
        re.search(r'^rust-version\s*=\s*"([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.M).group(1)
    )
    rustc = run(["rustc", "--version"], env)
    have = version_tuple(rustc)
    if have is None:
        problems.append("Rust is not installed: brew install rustup && rustup default stable")
    elif have < needed:
        problems.append(f"Rust {have[0]}.{have[1]} is older than {needed[0]}.{needed[1]}: rustup update")
    if run(["cargo", "about", "--version"], env) is None:
        problems.append("no cargo-about: cargo install --locked cargo-about --features cli")
    if run(["xcrun", "--find", "codesign"], env) is None:
        problems.append("no Xcode command-line tools: xcode-select --install")

    identities = run(["security", "find-identity", "-v", "-p", "codesigning"], env) or ""
    if cfg["identity"].upper() not in identities.upper():
        problems.append(f"the signing certificate {cfg['identity']} is not in the keychain")

    if not (ROOT / "base-pack.toml").is_file():
        problems.append("no base-pack.toml: cargo xtask base-pack update pins one")

    if run(["git", "rev-parse", "--is-shallow-repository"], env) is None:
        problems.append(f"{ROOT} is not a git checkout")
    else:
        dirty = run(["git", "status", "--porcelain", "--untracked-files=no"], env)
        if dirty and not allow_dirty:
            problems.append("uncommitted changes to tracked files (the packager refuses them):")
            problems.extend("    " + line for line in dirty.splitlines()[:10])
    return problems


def keep_out_of_dropbox(paths):
    """Mark each folder ignored by Dropbox, making it first so nothing in it syncs."""
    for p in paths:
        p.mkdir(parents=True, exist_ok=True)
        subprocess.run(["xattr", "-w", DROPBOX_IGNORE, "1", str(p)], check=True)


def unpack_outputs(stage, out):
    """Unpack the packager's archives from `stage` into `out`, replacing earlier ones."""
    out.mkdir(parents=True, exist_ok=True)
    unpacked = []
    for archive in sorted(stage.glob("*.tar.gz")):
        with tarfile.open(archive) as tar:
            tops = {Path(name).parts[0] for name in tar.getnames()}
            for top in tops:
                if (out / top).exists():
                    shutil.rmtree(out / top)
            tar.extractall(out)
        unpacked.extend(out / top for top in tops)
    for image in sorted(stage.glob("*.dmg")):
        # The app is copied off the disk image, and the image kept beside it to share.
        mount = Path(tempfile.mkdtemp(prefix="pyreal-dmg-"))
        subprocess.run(["hdiutil", "attach", "-readonly", "-nobrowse", "-noautoopen",
                        "-mountpoint", str(mount), str(image)], check=True, stdout=subprocess.DEVNULL)
        try:
            for app in sorted(mount.glob("*.app")):
                if (out / app.name).exists():
                    shutil.rmtree(out / app.name)
                subprocess.run(["ditto", str(app), str(out / app.name)], check=True)
                unpacked.append(out / app.name)
        finally:
            subprocess.run(["hdiutil", "detach", str(mount), "-force"], check=False,
                           stdout=subprocess.DEVNULL)
            mount.rmdir()
        shutil.copy2(image, out / image.name)
        unpacked.append(out / image.name)
    return unpacked


def package(product, stage, cfg, env, allow_dirty, extra=()):
    """Run the project's packager for `product`, writing into a new `stage`."""
    shutil.rmtree(stage, ignore_errors=True)
    command = ["cargo", "xtask", "package", product, "--out", str(stage), *extra]
    if allow_dirty:
        command.append("--allow-dirty")
    status = subprocess.run(command, cwd=ROOT, env=env).returncode
    if status != 0:
        raise Failed(f"the {product} package build failed (exit {status})")


def strike_emblem(cfg, env):
    """Render the icon with build.ini's emblem, and have the packager take it."""
    emblem = cfg["icon_emblem"]
    if emblem is None:
        return
    if not emblem.is_file():
        raise Failed(f"icon_emblem {emblem} is not a file")
    icon = ROOT / ICON_STAGE
    shutil.rmtree(icon, ignore_errors=True)
    renderer = ROOT / "pyreal/client/assets/render-icon.py"
    command = [sys.executable, str(renderer), "--emblem", str(emblem), "--out", str(icon)]
    if subprocess.run(command, cwd=ROOT, env=env).returncode != 0:
        raise Failed(f"the icon with {emblem.name} did not render")
    env["PYREAL_ICON_DIR"] = str(icon)


def build_base_pack(env):
    """Make the pinned base pack, or find it already made."""
    if subprocess.run(["cargo", "xtask", "base-pack", "build"], cwd=ROOT, env=env).returncode != 0:
        raise Failed("the pinned base pack did not build")


def build_client(out, cfg, env, allow_dirty):
    build_base_pack(env)
    strike_emblem(cfg, env)
    stage = ROOT / STAGE / "pyreal"
    package("pyreal", stage, cfg, env, allow_dirty, ["--base-pack", str(ROOT / BASE_PACK)])
    apps = [p for p in unpack_outputs(stage, out) if p.suffix == ".app"]
    if not apps:
        raise Failed(f"no .app among the archives in {stage}")
    app = apps[0]
    print(f"\nBuilt: {app}")
    verify(app, cfg, env)
    if cfg["applications_dir"]:
        install(app, cfg["applications_dir"])


def verify(app, cfg, env):
    """Check that `app` is Pyreal with the base pack and no MoltenVK, and its signature as macOS will."""
    plist = run(["/usr/libexec/PlistBuddy", "-c", "Print :CFBundleIdentifier", str(app / "Contents/Info.plist")], env)
    if (plist or "").strip() != BUNDLE_ID:
        raise Failed(f"{app.name} is not {BUNDLE_ID} (its Info.plist says {plist!r})")
    base = app / "Contents/Resources" / Path(BASE_PACK).name
    others = [p for p in app.rglob("*.dat") if p != base]
    if others:
        raise Failed(f"{app.name} carries a data file other than the base pack: {others[0]}")
    if not base.is_file() or base.stat().st_size != (ROOT / BASE_PACK).stat().st_size:
        raise Failed(f"{app.name} does not carry {BASE_PACK} as {base}")
    moltenvk = [p for p in app.rglob("*") if "moltenvk" in p.name.lower()]
    if moltenvk:
        raise Failed(f"{app.name} still carries MoltenVK: {moltenvk[0]}")
    if run(["codesign", "--verify", "--deep", "--strict", str(app)], env) is None:
        raise Failed(f"{app.name} does not verify: codesign --verify --deep --strict -v {app}")
    details = subprocess.run(
        ["codesign", "-dv", "--verbose=2", str(app)], text=True, capture_output=True, env=env
    ).stderr
    shown = [l for l in details.splitlines() if l.startswith(("Authority=", "TeamIdentifier=", "CodeDirectory"))]
    print("\n".join("  " + l for l in shown[:4]))
    if "flags=0x10000(runtime)" not in details:
        raise Failed(f"{app.name} is not signed with the hardened runtime, so not with {cfg['identity']}")


def install(app, folder):
    dest = Path(os.path.expanduser(folder)) / app.name
    if dest.exists():
        shutil.rmtree(dest)
    shutil.copytree(app, dest, symlinks=True)
    print(f"Installed {dest}")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="check only, build nothing")
    parser.add_argument("--allow-dirty", action="store_true", help="build uncommitted changes too")
    args = parser.parse_args()

    cfg = load_config()
    env = build_env(cfg)

    problems = check(cfg, env, args.allow_dirty)
    if problems:
        raise Failed("cannot build yet:\n  " + "\n  ".join(problems))
    commit = run(["git", "rev-parse", "--short=8", "HEAD"], env).strip()
    out = cfg["out_dir"] / commit
    keep_out_of_dropbox([ROOT / d for d in BUILD_DIRS] + [cfg["out_dir"]])
    print(f"Ready: commit {commit}, signing with {cfg['identity']}, outputs to {out}")
    if args.check:
        return

    build_client(out, cfg, env, args.allow_dirty)
    shutil.rmtree(ROOT / STAGE, ignore_errors=True)
    shutil.rmtree(ROOT / ICON_STAGE, ignore_errors=True)
    print("Done.")


if __name__ == "__main__":
    try:
        main()
    except Failed as e:
        sys.exit(f"build.py: {e}")
    except KeyboardInterrupt:
        sys.exit(130)
