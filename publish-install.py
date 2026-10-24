#!/usr/bin/env python3
"""Publish doomify-waw, then install the executable just published as the doomify-waw command.

usage: python3 publish-install.py [--accept] [--no-build | --source] [--dir DIR]

First it says, under a red warning, exactly what it will do, and asks before doing any of it
(--accept goes ahead without asking). Then it builds the release with publish.py (doomify-waw for
Windows and as an AppImage, into published/releases/), or with --source only doomify-waw for this
machine (cargo build --release), or with --no-build nothing at all; and copies it to DIR as
doomify-waw: by default ~/.local/bin, or %LOCALAPPDATA%\\Programs\\doomify-waw on Windows. With it
on the PATH, a map's doomify.py uses it when there is no doomify-waw checkout beside the map.

Dependency-free Python 3.8+.
"""

import argparse
import os
import platform
import shutil
import subprocess
import sys
import textwrap

HERE = os.path.dirname(os.path.abspath(__file__))
RELEASES = os.path.join(HERE, "published", "releases")
WINDOWS = platform.system() == "Windows"
EXE = ".exe" if WINDOWS else ""


def default_dir():
    if WINDOWS:
        return os.path.join(os.environ["LOCALAPPDATA"], "Programs", "doomify-waw")
    return os.path.join(os.path.expanduser("~"), ".local", "bin")


def run(command):
    print("+", " ".join(command), flush=True)
    subprocess.run(command, check=True, cwd=HERE)


def warning(lines):
    """`lines` under a red '--- Warning ---' (plain where the output isn't a terminal)."""
    if WINDOWS:
        os.system("")  # lets the Windows console show colors
    color = sys.stdout.isatty() and "NO_COLOR" not in os.environ
    print("\033[31m--- Warning ---\033[0m" if color else "--- Warning ---")
    width = min(shutil.get_terminal_size().columns, 100)
    for line in lines:
        # a step's later lines line up under its text
        hang = len(line) - len(line.lstrip(" -"))
        print(textwrap.fill(line, width, subsequent_indent=" " * hang,
                            break_on_hyphens=False, break_long_words=False) if line else "")
    print()


def agreed(accept):
    if accept:
        return True
    if not sys.stdin.isatty():
        sys.exit("There is no one to ask, so nothing was done: pass --accept to go ahead.")
    return input("Go ahead? [y/N] ").strip().lower() in ("y", "yes")


def plan(args):
    """What saying yes does, step by step."""
    target = os.path.join(args.dir, "doomify-waw" + EXE)
    lines = ["Saying yes will:"]
    if args.no_build:
        lines.append("  - build nothing, and install the doomify-waw published/releases/ has now")
    elif args.source:
        lines.append(f"  - build doomify-waw{EXE} for this machine (cargo build --release, against "
                     "../hell-hound) and copy it into published/releases/")
    else:
        lines.append("  - build doomify-waw's release into published/releases/, replacing what is "
                     "there: doomify-waw.exe cross-built for Windows (Rust's x86_64-pc-windows-gnu "
                     "target) and doomify-waw.AppImage (needs mksquashfs), both against ../hell-hound, "
                     "with LICENSE and THIRD_PARTY_LICENSES.txt (needs cargo-license)")
    lines.append(f"  - copy it to {target}, replacing any doomify-waw there")
    lines.append("")
    lines.append("To change that: --no-build installs what is published already, --source builds only "
                 "for this machine, and --dir says where it goes.")
    return lines


def build(args):
    if args.no_build:
        return
    if args.source:
        run(["cargo", "build", "--release"])
        os.makedirs(RELEASES, exist_ok=True)
        shutil.copy2(os.path.join(HERE, "target", "release", "doomify-waw" + EXE),
                     os.path.join(RELEASES, "doomify-waw" + EXE))
        return
    run([sys.executable, "publish.py"])
    # 🧹 a plain doomify-waw left by --source would be taken over the AppImage just built
    leftover = os.path.join(RELEASES, "doomify-waw")
    if not WINDOWS and os.path.isfile(leftover):
        os.remove(leftover)


def published():
    """The doomify-waw in published/releases/: the .exe on Windows, else the plain one --source
    built or the AppImage."""
    names = ["doomify-waw.exe"] if WINDOWS else ["doomify-waw", "doomify-waw.AppImage"]
    for name in names:
        path = os.path.join(RELEASES, name)
        if os.path.isfile(path):
            return path
    sys.exit("There is no doomify-waw in published/releases/: build it (leave out --no-build).")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--accept", action="store_true", help="go ahead without asking")
    build_how = parser.add_mutually_exclusive_group()
    build_how.add_argument("--no-build", action="store_true",
                           help="install what published/releases/ has now, building nothing")
    build_how.add_argument("--source", action="store_true",
                           help="build doomify-waw for this machine only (cargo build --release)")
    parser.add_argument("--dir", default=default_dir(),
                        help="where to put doomify-waw (default: %(default)s)")
    args = parser.parse_args()

    warning(plan(args))
    if not agreed(args.accept):
        sys.exit("Nothing was done.")
    build(args)
    source = published()
    target = os.path.join(args.dir, "doomify-waw" + EXE)
    os.makedirs(args.dir, exist_ok=True)
    # Copied beside it, then moved over it, so a running doomify-waw isn't overwritten in place
    shutil.copy2(source, target + ".part")
    os.replace(target + ".part", target)
    print("Installed", target)
    found = shutil.which("doomify-waw")
    if found is None or os.path.abspath(found) != os.path.abspath(target):
        print(f"{args.dir} isn't first on your PATH for doomify-waw" +
              (f" ({found} is)" if found else "") + ": add it to use this one.")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        sys.exit(f"publish-install failed: {error}")
