#!/usr/bin/env python3
"""Publish doomify-waw, then install the executable just published as the doomify-waw command.

usage: python3 publish-install.py [--accept] [--no-build | --source] [--dir DIR] [--no-shortcuts]

First it says, under a red warning, exactly what it will do, and asks before doing any of it
(--accept goes ahead without asking). Then it builds the release with publish.py (doomify-waw for
Windows and as an AppImage, into published/releases/), or with --source only doomify-waw for this
machine (cargo build --release), or with --no-build nothing at all; and copies it to DIR as
doomify-waw: by default ~/.local/bin, or %LOCALAPPDATA%\\Programs\\doomify-waw on Windows. With it
on the PATH, a map's doomify.py uses it when there is no doomify-waw checkout beside the map. Once it
may go ahead, it asks whether to make a shortcut (the app menu and the desktop, or the Start
menu and the desktop on Windows) with the system's lock icon, which opens a terminal with its
help; --no-shortcuts makes none without asking.

Dependency-free Python 3.8+.
"""

import argparse
import os
import platform
import shutil
import subprocess
import sys
import textwrap
import time

HERE = os.path.dirname(os.path.abspath(__file__))
RELEASES = os.path.join(HERE, "published", "releases")
WINDOWS = platform.system() == "Windows"
WHAT = "doomify-waw"
# The installer it runs, which takes --computer-speak too
INSTALLER = None
EXE = ".exe" if WINDOWS else ""


# The journal: everything this does, with the time since it began (what it plans and is told,
# every command and all it prints, every file it puts somewhere), added to JOURNAL on each run.
JOURNAL = os.path.join(HERE, "published", "artifacts", "publish-install.log")
BEGAN = time.monotonic()
# Blender's line for every frame it renders, which would bury the rest
CHATTER = ("Fra:", "Saved:", "Time:", "Read blend")


def note(line):
    """Writes `line` down in the journal."""
    os.makedirs(os.path.dirname(JOURNAL), exist_ok=True)
    with open(JOURNAL, "a", encoding="utf-8") as journal:
        for part in line.splitlines() or [""]:
            journal.write(f"[{time.monotonic() - BEGAN:8.3f}s] {part}\n")


# The voice it speaks in, from installation-groans: the workspace's checkout beside this project,
# else GitHub. Toiling Through Hell unless told --computer-speak; without one (or Python older than
# 3.11, which can't read TOML), plain lines.
GROANS = "https://raw.githubusercontent.com/error-error-fruit-user/installation-groans/main/"
VOICE = {}


def speak_in(name):
    """Speaks in the installation-groans voice `name` (a file name) from now on, if it is found."""
    global VOICE
    try:
        import tomllib
    except ImportError:
        return
    path = os.path.join(os.path.dirname(HERE), "installation-groans", name)
    try:
        if os.path.isfile(path):
            with open(path, encoding="utf-8") as source:
                text = source.read()
        else:
            import urllib.request
            with urllib.request.urlopen(GROANS + name, timeout=5) as answer:
                text = answer.read().decode()
        VOICE = tomllib.loads(text)
    except Exception:
        VOICE = {}


def tell(event, plain, **fills):
    """Says `event` as one of the voice's phrases for it, picked at random and filled from `fills`,
    or `plain` without one; the journal keeps the plain line either way. Now and then the voice
    interjects (its [interjection])."""
    import random
    phrases = VOICE.get("phrases", {}).get(event) or []
    if not phrases:
        print(plain, flush=True)
        note(plain)
        return
    said = random.choice(phrases)
    for name, value in fills.items():
        said = said.replace("{" + name + "}", str(value))
    said = said[:1].upper() + said[1:]
    print(said, flush=True)
    note(f"{said}  ({plain})")
    interjection = VOICE.get("interjection", {})
    if interjection.get("phrases") and random.random() < float(interjection.get("chance", 0)):
        colors = {"yellow": 33, "green": 32, "blue": 34, "magenta": 35, "cyan": 36, "white": 37}
        code = colors.get(interjection.get("color", "red"), 31)
        shout = random.choice(interjection["phrases"])
        tty = sys.stdout.isatty() and "NO_COLOR" not in os.environ
        print(f"\033[1;{code}m{shout}\033[0m" if tty else shout, flush=True)
        note(shout)


def not_mine(folders):
    """The first file in `folders` (and the folders themselves) owned by someone else, as a run
    with sudo leaves them; None when every one is this user's."""
    if WINDOWS:
        return None
    for folder in folders:
        for root, dirs, files in os.walk(folder):
            for path in [root] + [os.path.join(root, name) for name in dirs + files]:
                try:
                    if os.lstat(path).st_uid != os.getuid():
                        return path
                except OSError:
                    pass
    return None


def check_owner(folders):
    """Stops, saying how to take them back, when anything in `folders` isn't this user's."""
    if WINDOWS or os.getuid() == 0:
        return
    stranger = not_mine(folders)
    if stranger is not None:
        import getpass
        fix = "sudo chown -R " + getpass.getuser() + " " + " ".join(folders)
        tell("problem", f"{stranger} isn't yours (a run with sudo left it)", problem=f"{stranger} isn't yours")
        sys.exit(f"Something in published/ belongs to another user, root most likely (a run with sudo "
                 f"leaves it): nothing can replace it. Take it back with:\n  {fix}")


def run(command, cwd=HERE):
    """Runs `command`, its output shown as it comes (prompts included) and written down, with how it
    ended. Raises CalledProcessError if it failed."""
    shown = " ".join(str(part) for part in command)
    tell("run", f"+ {shown}", command=shown)
    note(f"+ {shown}  (in {cwd})")
    process = subprocess.Popen(command, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    pending, quiet = b"", 0
    while True:
        chunk = process.stdout.read1(4096)
        if not chunk:
            break
        sys.stdout.buffer.write(chunk)
        sys.stdout.buffer.flush()
        *lines, pending = (pending + chunk).split(b"\n")
        for line in lines:
            text = line.decode(errors="replace").rstrip("\r")
            if text.startswith(CHATTER):
                quiet += 1
            else:
                note("  | " + text)
    if pending:
        note("  | " + pending.decode(errors="replace"))
    code = process.wait()
    if quiet:
        note(f"  ({quiet} lines of Blender's frame by frame progress left out)")
    note(f"  ended with code {code}")
    if code:
        raise subprocess.CalledProcessError(code, command)


def default_dir():
    if WINDOWS:
        return os.path.join(os.environ["LOCALAPPDATA"], "Programs", "doomify-waw")
    return os.path.join(os.path.expanduser("~"), ".local", "bin")



def warning(lines):
    """`lines` under a red '--- Warning ---' (plain where the output isn't a terminal)."""
    if WINDOWS:
        os.system("")  # lets the Windows console show colors
    color = sys.stdout.isatty() and "NO_COLOR" not in os.environ
    print("\033[31m--- Warning ---\033[0m" if color else "--- Warning ---")
    note("--- Warning ---")
    for line in lines:
        note(line)
    width = min(shutil.get_terminal_size().columns, 100)
    for line in lines:
        # a step's later lines line up under its text
        hang = len(line) - len(line.lstrip(" -"))
        print(textwrap.fill(line, width, subsequent_indent=" " * hang,
                            break_on_hyphens=False, break_long_words=False) if line else "")
    print()


def agreed(accept):
    if accept:
        note("--accept: going ahead without asking")
        return True
    if not sys.stdin.isatty():
        note("no one to ask, and no --accept: nothing was done")
        sys.exit("There is no one to ask, so nothing was done: pass --accept to go ahead.")
    answer = input("Are you sure? [y/n] ").strip()
    note(f"asked to go ahead, the answer was {answer!r}")
    return answer.lower() in ("y", "yes")



# The system's lock icon for every shortcut: the icon theme's on Linux, shell32.dll's padlock on Windows
LOCK_ICON = "system-lock-screen"
WINDOWS_LOCK_ICON = r"%SystemRoot%\System32\shell32.dll,47"


def shortcut_places():
    """Where shortcuts go on Linux: the app menu, and the desktop if there is one."""
    home = os.path.expanduser("~")
    data = os.environ.get("XDG_DATA_HOME") or os.path.join(home, ".local", "share")
    places = [os.path.join(data, "applications")]
    try:
        desktop = subprocess.run(["xdg-user-dir", "DESKTOP"], capture_output=True,
                                 text=True).stdout.strip()
    except OSError:
        desktop = ""
    desktop = desktop or os.path.join(home, "Desktop")
    if os.path.isdir(desktop) and os.path.abspath(desktop) != os.path.abspath(home):
        places.append(desktop)
    return places


def wants_shortcuts(args, what):
    """Asks, once it may go ahead, whether to make shortcuts for `what` (--no-shortcuts: no,
    without asking)."""
    if args.no_shortcuts:
        note("--no-shortcuts: no shortcuts, and no question about them")
        return False
    if not sys.stdin.isatty():
        note("no one to ask about shortcuts: none are made")
        return False
    if WINDOWS:
        where = "your Start menu and desktop"
    else:
        where = " and ".join(place.replace(os.path.expanduser("~"), "~", 1)
                             for place in shortcut_places())
    answer = input(f"Make shortcuts for {what} in {where}, with the system's lock icon? "
                   "[y/n] ").strip()
    note(f"asked about shortcuts for {what}, the answer was {answer!r}")
    return answer.lower() in ("y", "yes")


def exec_quoted(arg):
    """`arg` as a desktop entry's Exec line wants it."""
    if arg and not any(c in arg for c in " \t\n\"'\\><~|&;$*?#()`"):
        return arg
    return '"' + "".join("\\" + c if c in '"`$\\' else c for c in arg) + '"'


def make_shortcut(title, file_id, command, comment):
    """A shortcut called `title` that runs `command` in a terminal, with the lock icon: a desktop
    entry `file_id`.desktop in each of `shortcut_places()` on Linux, a .lnk in the Start menu and on
    the desktop on Windows."""
    if WINDOWS:
        def ps(text):
            return "'" + text.replace("'", "''") + "'"
        script = (
            "$shell = New-Object -ComObject WScript.Shell\n"
            "foreach ($folder in @([Environment]::GetFolderPath('Programs'), "
            "[Environment]::GetFolderPath('Desktop'))) {\n"
            f"  $link = $shell.CreateShortcut((Join-Path $folder {ps(title + '.lnk')}))\n"
            f"  $link.TargetPath = {ps(command[0])}\n"
            f"  $link.Arguments = {ps(subprocess.list2cmdline(command[1:]))}\n"
            f"  $link.WorkingDirectory = {ps(os.path.dirname(os.path.abspath(command[0])))}\n"
            f"  $link.Description = {ps(comment)}\n"
            f"  $link.IconLocation = {ps(WINDOWS_LOCK_ICON)}\n"
            "  $link.Save()\n"
            "  Write-Output (Join-Path $folder " + ps(title + ".lnk") + ")\n"
            "}\n")
        run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script])
        return
    exec_line = " ".join(exec_quoted(arg) for arg in command)
    entry = ("[Desktop Entry]\nType=Application\n"
             f"Name={title}\nComment={comment}\n"
             f"Exec={exec_line.replace(chr(92), chr(92) * 2).replace('%', '%%')}\n"
             f"Icon={LOCK_ICON}\nTerminal=true\nCategories=Game;\n")
    for place in shortcut_places():
        os.makedirs(place, exist_ok=True)
        path = os.path.join(place, file_id + ".desktop")
        with open(path, "w", encoding="utf-8") as file:
            file.write(entry)
        os.chmod(path, 0o755)
        # a desktop launches only the shortcuts it is told to trust
        if not path.startswith(shortcut_places()[0]) and shutil.which("gio"):
            subprocess.run(["gio", "set", path, "metadata::trusted", "true"],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        note(f"made the shortcut {path}: {exec_line}")
        print("Shortcut:", path)


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
                 "for this machine, --dir says where it goes, and --no-shortcuts doesn't ask about "
                 "shortcuts.")
    return lines


def build(args):
    if args.no_build:
        return
    if args.source:
        run(["cargo", "build", "--release"])
        os.makedirs(RELEASES, exist_ok=True)
        shutil.copy2(os.path.join(HERE, "target", "release", "doomify-waw" + EXE),
                     os.path.join(RELEASES, "doomify-waw" + EXE))
        note(f"copied the release build into {RELEASES}")
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


def rust_on_path():
    """Puts rustup's folder ($CARGO_HOME/bin, else ~/.cargo/bin) on the PATH when cargo isn't on
    it, as a shell that never ran rustup's profile line leaves it; everything this runs inherits it."""
    if shutil.which("cargo"):
        return
    home = os.environ.get("CARGO_HOME") or os.path.join(os.path.expanduser("~"), ".cargo")
    folder = os.path.join(home, "bin")
    if any(os.path.isfile(os.path.join(folder, name)) for name in ("cargo", "cargo.exe")):
        os.environ["PATH"] = folder + os.pathsep + os.environ.get("PATH", "")


def main():
    rust_on_path()
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--accept", action="store_true", help="go ahead without asking")
    build_how = parser.add_mutually_exclusive_group()
    build_how.add_argument("--no-build", action="store_true",
                           help="install what published/releases/ has now, building nothing")
    build_how.add_argument("--source", action="store_true",
                           help="build doomify-waw for this machine only (cargo build --release)")
    parser.add_argument("--dir", default=default_dir(),
                        help="where to put doomify-waw (default: %(default)s)")
    parser.add_argument("--no-shortcuts", action="store_true",
                        help="make no shortcuts, and don't ask about them")
    parser.add_argument("--computer-speak", action="store_true",
                        help="say what it does plainly and formally, rather than as a descent "
                             "through hell" + (" (and have the installer do the same)" if INSTALLER else ""))
    args = parser.parse_args()
    speak_in("computer-speak.toml" if args.computer_speak else "toiling-through-hell.toml")
    note(f"=== {os.path.basename(__file__)} {' '.join(sys.argv[1:])} ===")

    warning(plan(args))
    if not agreed(args.accept):
        note("not going ahead: nothing was done")
        sys.exit("Nothing was done.")
    shortcuts = wants_shortcuts(args, "doomify-waw")
    check_owner([folder for folder in (os.path.normpath(os.path.join(HERE, "published")),) if os.path.isdir(folder)])
    tell("begin", f"Publishing and installing {WHAT}", what=WHAT, place=os.path.join(HERE, "published"))
    build(args)
    source = published()
    target = os.path.join(args.dir, "doomify-waw" + EXE)
    os.makedirs(args.dir, exist_ok=True)
    # Copied beside it, then moved over it, so a running doomify-waw isn't overwritten in place
    shutil.copy2(source, target + ".part")
    os.replace(target + ".part", target)
    note(f"copied {source} to {target}")
    tell("copy", f"Installed {target}", what=os.path.basename(target), place=target)
    found = shutil.which("doomify-waw")
    if found is None or os.path.abspath(found) != os.path.abspath(target):
        print(f"{args.dir} isn't first on your PATH for doomify-waw" +
              (f" ({found} is)" if found else "") + ": add it to use this one.")
    if shortcuts:
        # 📜 it works on a map's folder, so its shortcut opens a terminal with its help
        if WINDOWS:
            command = [os.environ.get("ComSpec", "cmd.exe"), "/k", target, "--help"]
        else:
            command = ["sh", "-c", f"'{target}' --help; echo; echo 'Press Enter to close.'; read _"]
        make_shortcut("Doomify WaW", "doomify-waw", command,
                      "Rebuilds World at War's weapons for UZDoom maps (shows its help)")


if __name__ == "__main__":
    try:
        main()
        note("done")
    except subprocess.CalledProcessError as error:
        note(f"failed: {error}")
        tell("problem", f"publish-install failed: {error}", problem=error)
        tell("journal", f"What it did is in {JOURNAL}", place=JOURNAL)
        sys.exit(1)
    tell("done", "Done.", what=WHAT)
    tell("journal", f"What it did is in {JOURNAL}", place=JOURNAL)
