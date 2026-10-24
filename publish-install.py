#!/usr/bin/env python3
"""Publish doomify-waw, then install the executable just published as the doomify-waw command.

usage: python3 publish-install.py [--dir DIR]

Runs publish.py (published/releases/), then copies doomify-waw.AppImage (doomify-waw.exe on
Windows) to DIR as doomify-waw: by default ~/.local/bin, or %LOCALAPPDATA%\\Programs\\doomify-waw
on Windows. With it on the PATH, the maps' doomify.py use it rather than building their own.

Dependency-free Python 3.8+.
"""

import argparse
import os
import platform
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
RELEASES = os.path.join(HERE, "published", "releases")
WINDOWS = platform.system() == "Windows"


def default_dir():
    if WINDOWS:
        return os.path.join(os.environ["LOCALAPPDATA"], "Programs", "doomify-waw")
    return os.path.join(os.path.expanduser("~"), ".local", "bin")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--dir", default=default_dir(),
                        help="where to put doomify-waw (default: %(default)s)")
    args = parser.parse_args()

    command = [sys.executable, "publish.py"]
    print("+", " ".join(command), flush=True)
    subprocess.run(command, check=True, cwd=HERE)

    source = os.path.join(RELEASES, "doomify-waw.exe" if WINDOWS else "doomify-waw.AppImage")
    target = os.path.join(args.dir, "doomify-waw.exe" if WINDOWS else "doomify-waw")
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
