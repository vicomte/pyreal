#!/usr/bin/env python3
"""The machine lock's heavy hold for Python and shell tools, on the files the crate beside it uses.

    python3 machine_lock.py heavy [--what TEXT] -- <command> [args]   runs <command> under a heavy hold

build.py imports it for hold_heavy. A heavy hold never waits: it is a shared flock on machine.modes,
taken only while no quiet run holds machine.queue, and a note in machine.holders kept locked while
its holder lives, so a quiet run knows to wait for it (see src/lib.rs). PYREAL_MACHINE_LOCK=off,
or a parent's hold, skips it.
"""

import atexit
import fcntl
import itertools
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

ENV = "PYREAL_MACHINE_LOCK"
# Values that skip the lock: off by choice, or a parent's hold.
SKIP = ("off", "held", "heavy", "quiet")
_ids = itertools.count()


def cache_dir():
    """Pyreal's per-user cache folder, as the crate's cache_dir finds it."""
    if sys.platform == "darwin":
        return Path.home() / "Library" / "Caches" / "pyreal"
    return Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache") / "pyreal"


def _try(f, how):
    try:
        fcntl.flock(f, how | fcntl.LOCK_NB)
        return True
    except BlockingIOError:
        return False


def _parse(text):
    return dict(line.split("=", 1) for line in text.splitlines() if "=" in line)


def live_notes(folder):
    """The notes of live holders and waiting quiet runs, oldest first; dead holders' are removed."""
    notes = []
    for path in sorted((folder / "machine.holders").glob("*.note")):
        try:
            f = open(path, "rb")
        except OSError:
            continue
        with f:
            if _try(f, fcntl.LOCK_EX):
                path.unlink(missing_ok=True)
                continue
            note = _parse(f.read().decode(errors="replace"))
        if "pid" in note and "what" in note:
            notes.append(note)
    return sorted(notes, key=lambda n: (int(n.get("since", 0)), int(n.get("pid", 0))))


def describe(note, now):
    """One line for a person, as the crate's Holder::describe writes it."""
    age = max(0, now - int(note.get("since", now)))
    if note.get("mode") == "heavy":
        how = "heavy, beside a quiet run" if note.get("beside_quiet") == "1" else "heavy"
    else:
        how = "waiting for quiet" if note.get("waiting") == "1" else "quiet"
    return (f"`{note.get('what')}` (pid {note.get('pid')}, {how}) in {note.get('dir', '?')}, "
            f"for {age // 60}m {age % 60:02d}s")


class Hold:
    """A heavy hold, let go by release() or when the process exits."""

    def __init__(self, lock, note, note_path):
        self.lock, self.note, self.note_path = lock, note, note_path

    def release(self):
        if self.note is not None:
            self.note_path.unlink(missing_ok=True)
            self.note.close()
            self.note = None
            if self.lock is not None:
                self.lock.close()


def _place_note(folder, what, where, beside_quiet):
    notes = folder / "machine.holders"
    notes.mkdir(parents=True, exist_ok=True)
    path = notes / f"{os.getpid()}-{next(_ids)}.note"
    tmp = path.with_suffix(".tmp")
    f = open(tmp, "w")
    f.write(f"pid={os.getpid()}\nwhat={what}\ndir={where}\nsince={int(time.time())}\n"
            f"mode=heavy\nwaiting=0\nbeside_quiet={int(beside_quiet)}\n")
    f.flush()
    fcntl.flock(f, fcntl.LOCK_SH)
    os.replace(tmp, path)
    return f, path


def _stderr(line):
    print(line, file=sys.stderr, flush=True)


def hold_heavy(what, env=None, where=None, say=_stderr):
    """Takes a heavy hold for `what` without waiting, and sets PYREAL_MACHINE_LOCK=heavy in `env`
    (os.environ by default) for the processes it starts. When a quiet run holds the machine or is
    next, it takes no share and says which run it may skew. None when `env` says to skip it."""
    env = os.environ if env is None else env
    if env.get(ENV) in SKIP:
        return None
    folder = cache_dir()
    folder.mkdir(parents=True, exist_ok=True)
    lock = open(folder / "machine.modes", "a")
    with open(folder / "machine.queue", "a") as queue:
        shared = _try(queue, fcntl.LOCK_SH) and _try(lock, fcntl.LOCK_SH)
    if not shared:
        lock.close()
        lock = None
        quiet = [n for n in live_notes(folder) if n.get("mode", "quiet") == "quiet"]
        who = describe(quiet[0], int(time.time())) if quiet else "a quiet run"
        say(f"the machine lock: `{what}` runs beside {who} and may skew its timings")
    hold = Hold(lock, *_place_note(folder, what, where or os.getcwd(), not shared))
    atexit.register(hold.release)
    env[ENV] = "heavy"
    return hold


def main(argv):
    if argv[:1] == ["heavy"]:
        rest, what = argv[1:], None
        if rest[:1] == ["--what"] and len(rest) > 1:
            what, rest = rest[1], rest[2:]
        if rest[:1] == ["--"] and len(rest) > 1:
            command = rest[1:]
            hold_heavy(what or " ".join(command))
            try:
                proc = subprocess.Popen(command)
            except OSError as e:
                _stderr(f"machine_lock.py: could not start {command[0]}: {e}")
                return 127
            # Ctrl-C reaches the command itself; a terminate is passed on to it.
            signal.signal(signal.SIGINT, signal.SIG_IGN)
            signal.signal(signal.SIGTERM, lambda *_: proc.terminate())
            code = proc.wait()
            return code if code >= 0 else 128 - code
    _stderr(__doc__.split("\n\n")[1])
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
