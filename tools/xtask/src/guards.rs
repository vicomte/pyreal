//! What the heavy tiers check before they start: that the Mac is on mains power, and their hold on
//! the machine lock.
//!
//! Tier 0, tier 1 and tier 2 refuse to run on battery unless `--on-battery` or
//! `PYREAL_ON_BATTERY=1` says to; `ci prepush` runs anyway. Tier 1, tier 2, the gates and `golden`
//! take a heavy hold on the machine lock (`pyreal-machine-lock`), as `build.py` and engine-view's
//! headless runs do: a heavy hold never waits and only marks the machine busy. `cargo xtask lock`
//! says who holds it, and `cargo xtask lock [--heavy|--quiet] -- <command>` runs a command holding
//! it; `--quiet`, for timing runs, waits until the heavy work before it has finished.

use std::process::Command;

use pyreal_machine_lock::{MachineLock, Mode};

/// The variable that lets a heavy tier run on battery.
pub(crate) const ON_BATTERY_ENV: &str = "PYREAL_ON_BATTERY";

/// Whether `pmset -g batt`'s output says the Mac draws from its battery.
fn drawing_from_battery(pmset: &str) -> bool {
    pmset
        .lines()
        .next()
        .is_some_and(|l| l.contains("'Battery Power'"))
}

/// Whether this machine runs on its battery. Only macOS is asked; elsewhere it is never.
fn on_battery() -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    Command::new("pmset")
        .args(["-g", "batt"])
        .output()
        .is_ok_and(|out| drawing_from_battery(&String::from_utf8_lossy(&out.stdout)))
}

/// Whether `args` or the environment allow a heavy run on battery.
fn battery_allowed(args: &[String], env: Option<&str>) -> bool {
    args.iter().any(|a| a == "--on-battery") || env == Some("1")
}

/// `Err` with the reason when `what` must not run now because the Mac is on battery.
pub(crate) fn mains_power(what: &str, args: &[String]) -> Result<(), String> {
    let env = std::env::var(ON_BATTERY_ENV).ok();
    if battery_allowed(args, env.as_deref()) || !on_battery() {
        return Ok(());
    }
    Err(format!(
        "{what} is not run on battery: plug the Mac in, or pass --on-battery \
         (or set {ON_BATTERY_ENV}=1) to run it anyway"
    ))
}

/// Takes a `mode` hold on the machine lock for `what`; only a quiet hold waits.
pub(crate) fn machine_lock(mode: Mode, what: &str) -> Result<Option<MachineLock>, String> {
    pyreal_machine_lock::acquire(mode, what).map_err(|e| format!("the machine lock: {e}"))
}

const LOCK_USAGE: &str = "usage: cargo xtask lock [[--heavy|--quiet] -- <command> [args]]";

/// What `cargo xtask lock` was asked: the status, or a command to run in a mode.
fn lock_request(args: &[String]) -> Result<Option<(Mode, &[String])>, String> {
    let (mode, rest) = match args.first().map(String::as_str) {
        None => return Ok(None),
        Some("--heavy") => (Mode::Heavy, &args[1..]),
        Some("--quiet") => (Mode::Quiet, &args[1..]),
        Some(_) => (Mode::Heavy, args),
    };
    match rest.split_first() {
        Some((dashes, command)) if dashes == "--" && !command.is_empty() => {
            Ok(Some((mode, command)))
        }
        _ => Err(LOCK_USAGE.to_owned()),
    }
}

/// `cargo xtask lock [[--heavy|--quiet] -- <command> [args]]`: who holds the machine lock, or
/// `command` run while holding it, heavy unless `--quiet`.
pub(crate) fn lock(args: &[String]) -> i32 {
    let (mode, command) = match lock_request(args) {
        Ok(None) => return lock_status(),
        Ok(Some(request)) => request,
        Err(usage) => {
            eprintln!("{usage}");
            return 2;
        }
    };
    let _lock = match machine_lock(mode, &command.join(" ")) {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("lock: {e}");
            return 1;
        }
    };
    match Command::new(&command[0]).args(&command[1..]).status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("lock: could not start {}: {e}", command[0]);
            1
        }
    }
}

fn lock_status() -> i32 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let state = match pyreal_machine_lock::state() {
        Ok(state) => state,
        Err(e) => {
            eprintln!("lock: {e}");
            return 1;
        }
    };
    if state.is_free() {
        println!("the machine lock is free");
        return 0;
    }
    println!("the machine lock is held by:");
    for h in &state.holders {
        println!("  {}", h.describe(now));
    }
    if state.unnamed {
        println!("  a process that left no note");
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The battery is read from pmset's first line**, the source the Mac draws from now.
    #[test]
    fn the_battery_is_read_from_pmsets_first_line() {
        assert!(drawing_from_battery(
            "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=1)\t80%; discharging\n"
        ));
        assert!(!drawing_from_battery(
            "Now drawing from 'AC Power'\n -InternalBattery-0 (id=1)\t100%; charged\n"
        ));
        assert!(!drawing_from_battery(""));
    }

    /// **`cargo xtask lock` runs a command heavy unless told `--quiet`**, and says how to call it
    /// otherwise.
    #[test]
    fn the_lock_command_takes_a_mode() {
        let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let cmd = args(&["make"]);
        assert_eq!(lock_request(&[]), Ok(None));
        assert_eq!(
            lock_request(&args(&["--", "make"])),
            Ok(Some((Mode::Heavy, &cmd[..])))
        );
        assert_eq!(
            lock_request(&args(&["--heavy", "--", "make"])),
            Ok(Some((Mode::Heavy, &cmd[..])))
        );
        assert_eq!(
            lock_request(&args(&["--quiet", "--", "make"])),
            Ok(Some((Mode::Quiet, &cmd[..])))
        );
        assert!(lock_request(&args(&["--quiet"])).is_err());
        assert!(lock_request(&args(&["--loud", "--", "make"])).is_err());
        assert!(lock_request(&args(&["make"])).is_err());
    }

    /// **A heavy tier runs on battery only when asked to**, by flag or by variable.
    #[test]
    fn a_heavy_tier_runs_on_battery_only_when_asked() {
        let none: [String; 0] = [];
        assert!(!battery_allowed(&none, None));
        assert!(!battery_allowed(&none, Some("0")));
        assert!(battery_allowed(&none, Some("1")));
        assert!(battery_allowed(&["--on-battery".to_owned()], None));
    }
}
