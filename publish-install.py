#!/usr/bin/env python3
"""Publish doomify-waw, then install the executable just published as the doomify-waw command.

usage: python3 publish-install.py [--accept] [--no-build | --source] [--dir DIR]
                                  [--desktop-shortcut] [--menu-entry] [--computer-speak]

First it says, under a red warning, exactly what it will do, and asks once: "Are you sure? [y/n]".
The answer can carry any of its flags after the y (`y --menu-entry`), which win over the same flags
on the command line; --accept goes ahead without asking, with what the command line says and the
defaults for the rest (no shortcuts among them). Then it builds the release with publish.py
(doomify-waw for Windows and as an AppImage, into published/releases/), or with --source only
doomify-waw for this machine (cargo build --release), or with --no-build nothing at all; and
copies it to DIR as doomify-waw: by default ~/.local/bin, or %LOCALAPPDATA%\\Programs\\doomify-waw
on Windows. With it on the PATH, a map's doomify.py uses it when there is no doomify-waw checkout
beside the map. --desktop-shortcut and --menu-entry make shortcuts with the system's lock icon,
which open a terminal with its help.

It speaks in a voice from ../installation-groans (Toiling Through Hell, or --computer-speak), and
everything it and what it runs did is in published/artifacts/publish-install.log.

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
JOURNAL = os.path.join(HERE, "published", "artifacts", "publish-install.log")
WINDOWS = platform.system() == "Windows"
EXE = ".exe" if WINDOWS else ""
GROANS_GITHUB = "https://raw.githubusercontent.com/error-error-fruit-user/installation-groans/main/"


def load_groans():
    """installation-groans' groans.py, which these scripts speak through: the project's
    installation-groans submodule, else the workspace's checkout beside the project, else a copy
    fetched from its GitHub into published/artifacts/groans/."""
    project = HERE
    places = [os.path.join(project, "installation-groans"),
              os.path.join(os.path.dirname(project), "installation-groans"),
              os.path.join(project, "published", "artifacts", "groans")]
    folder = next((place for place in places if os.path.isfile(os.path.join(place, "groans.py"))), None)
    if folder is None:
        folder = places[-1]
        try:
            import urllib.request
            os.makedirs(folder, exist_ok=True)
            for name in ("groans.py", "toiling-through-hell.toml", "computer-speak.toml"):
                with urllib.request.urlopen(GROANS_GITHUB + name, timeout=10) as answer:
                    with open(os.path.join(folder, name), "wb") as out:
                        out.write(answer.read())
        except Exception as error:
            sys.exit(f"installation-groans isn't beside {os.path.basename(project)}, and couldn't be "
                     f"fetched ({error}): clone it beside it.")
    if folder not in sys.path:
        sys.path.insert(0, folder)
    import groans
    return groans


groans = load_groans()


def rust_on_path():
    """Puts rustup's folder ($CARGO_HOME/bin, else ~/.cargo/bin) on the PATH when cargo isn't on
    it, as a shell that never ran rustup's profile line leaves it; everything this runs inherits it."""
    if shutil.which("cargo"):
        return
    home = os.environ.get("CARGO_HOME") or os.path.join(os.path.expanduser("~"), ".cargo")
    folder = os.path.join(home, "bin")
    if any(os.path.isfile(os.path.join(folder, name)) for name in ("cargo", "cargo.exe")):
        os.environ["PATH"] = folder + os.pathsep + os.environ.get("PATH", "")


def as_root():
    return not WINDOWS and os.geteuid() == 0


def check_owner(folders):
    """Stops, saying how to take them back, when anything in `folders` isn't this user's (a run
    with sudo leaves root's files behind)."""
    if WINDOWS or as_root():
        return
    folders = [folder for folder in folders if os.path.isdir(folder)]
    for folder in folders:
        for root, dirs, files in os.walk(folder):
            for path in [root] + [os.path.join(root, name) for name in dirs + files]:
                try:
                    stranger = os.lstat(path).st_uid != os.getuid()
                except OSError:
                    continue
                if stranger:
                    import getpass
                    fix = "sudo chown -R " + getpass.getuser() + " " + " ".join(folders)
                    groans.say("problem", f"{path} isn't yours (a run with sudo left it)",
                               problem=f"{path} isn't yours")
                    sys.exit(f"Something in published/ belongs to another user, root most likely: "
                             f"nothing can replace it. Take it back with:\n  {fix}")


def ask(flags, args, plan):
    """The warning, then the one question; returns the arguments, with any flags given with the
    y parsed after the command line's so they win. Stops if the answer is no."""
    groans.warning(plan(args))
    names = [name for action in flags._actions for name in action.option_strings
             if name not in ("-h", "--help", "--accept")]
    added = groans.confirm(args.accept, names)
    if added is None:
        groans.note("not going ahead: nothing was done")
        groans.declined()
    if added:
        groans.note(f"with the flags given at the question: {' '.join(added)}")
        return flags.parse_args(sys.argv[1:] + added)
    return args


def default_dir():
    if WINDOWS:
        return os.path.join(os.environ["LOCALAPPDATA"], "Programs", "doomify-waw")
    return os.path.join(os.path.expanduser("~"), ".local", "bin")


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
    for flag, where_to in (("desktop_shortcut", "a shortcut on your desktop"),
                           ("menu_entry", "an entry in your app menu")):
        if getattr(args, flag):
            lines.append(f"  - make {where_to} for doomify-waw, which opens a terminal with its help")
    lines.append("")
    lines.append("To change that, here or after the y: --no-build installs what is published already, "
                 "--source builds only for this machine, --dir says where it goes, "
                 "--desktop-shortcut and --menu-entry make shortcuts, --reinstall does it all again, and --computer-speak speaks "
                 "plainly.")
    return lines


def build(args):
    if args.no_build:
        return
    groans.section("build")
    if not args.reinstall and not args.source and groans.up_to_date([os.path.join(RELEASES, "doomify-waw" + (".exe" if WINDOWS else ".AppImage"))], [HERE, os.path.join(os.path.dirname(HERE), "hell-hound")]):
        groans.say("skip", "The release is newer than its sources: nothing to build "
                   "(--reinstall builds it again)", what="doomify-waw's release", place=RELEASES)
        return

    if args.source:
        groans.run(["cargo", "build", "--release"], cwd=HERE)
        os.makedirs(RELEASES, exist_ok=True)
        shutil.copy2(os.path.join(HERE, "target", "release", "doomify-waw" + EXE),
                     os.path.join(RELEASES, "doomify-waw" + EXE))
        groans.note(f"copied the release build into {RELEASES}")
        return
    groans.run([sys.executable, "publish.py"], cwd=HERE, voiced=True)
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


def parser():
    flags = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    flags.add_argument("--accept", action="store_true",
                       help="go ahead without asking, with the defaults for what isn't given")
    build_how = flags.add_mutually_exclusive_group()
    build_how.add_argument("--no-build", action="store_true",
                           help="install what published/releases/ has now, building nothing")
    build_how.add_argument("--source", action="store_true",
                           help="build doomify-waw for this machine only (cargo build --release)")
    flags.add_argument("--dir", default=default_dir(),
                       help="where to put doomify-waw (default: %(default)s)")
    flags.add_argument("--desktop-shortcut", action="store_true",
                       help="make a shortcut to doomify-waw on the desktop")
    flags.add_argument("--menu-entry", action="store_true",
                       help="make an entry for doomify-waw in the app menu (the Start menu on Windows)")
    flags.add_argument("--reinstall", action="store_true",
                       help="build and install everything again, where a run picks up where the last "
                            "one left off")
    flags.add_argument("--computer-speak", action="store_true",
                       help="say what it does plainly and formally, rather than as a descent through hell")
    return flags


def main():
    rust_on_path()
    flags = parser()
    args = flags.parse_args()
    groans.start(JOURNAL, args.computer_speak)
    if as_root():
        groans.note("run as root: refused")
        sys.exit("Don't run this with sudo: it installs for you, and root would leave files in "
                 "published/ and an install that only root can change.")
    args = ask(flags, args, plan)
    check_owner([os.path.join(HERE, "published")])
    build(args)
    groans.section("install")
    source = published()
    target = os.path.join(args.dir, "doomify-waw" + EXE)
    os.makedirs(args.dir, exist_ok=True)
    if not args.reinstall and groans.same_file(source, target):
        groans.say("skip", f"{target} is this doomify-waw already", what="doomify-waw", place=target)
    else:
        # Copied beside it, then moved over it, so a running doomify-waw isn't overwritten in place
        shutil.copy2(source, target + ".part")
        os.replace(target + ".part", target)
        groans.say("copy", f"Installed {target}", what="doomify-waw", place=target)
    found = shutil.which("doomify-waw")
    if found is None or os.path.abspath(found) != os.path.abspath(target):
        groans.other(f"note: {args.dir} isn't first on your PATH for doomify-waw" +
                     (f" ({found} is)" if found else "") + ": add it to use this one.")
    # 📜 it works on a map's folder, so its shortcut opens a terminal with its help
    if WINDOWS:
        command = [os.environ.get("ComSpec", "cmd.exe"), "/k", target, "--help"]
    else:
        command = ["sh", "-c", f"'{target}' --help; echo; echo 'Press Enter to close.'; read _"]
    groans.shortcut("Doomify WaW", "doomify-waw", command,
                    "Rebuilds World at War's weapons for UZDoom maps (shows its help)",
                    desktop=args.desktop_shortcut, menu=args.menu_entry)


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        groans.finish("doomify-waw", ok=False,
                      error=f"{' '.join(map(str, error.cmd))} ended with code {error.returncode}")
        sys.exit(1)
    groans.finish("doomify-waw")
