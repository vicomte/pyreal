//! `cargo xtask ci prepush`, the checks CI leaves to the machine a push comes from, and the git
//! hook that runs them before each push.
//!
//! CI runs the tests and clippy on Linux alone, a smoke set on Windows, and packages each system.
//! The Mac's build, tests and clippy run here, with the rules, the changelog and the docs first
//! because they fail a release in seconds, and clippy for Windows and Linux after the Mac's own. A
//! pass on a clean tree is recorded, so the hook does not run them again for the commit that
//! passed.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::{print_table, target_dir, workspace_root, Profile, Report};
use crate::{changelog, cross_clippy, Tier0Part};

/// Prepush's parts, cheapest first.
pub(crate) const PARTS: [Tier0Part; 5] = [
    Tier0Part::Rules,
    Tier0Part::Docs,
    Tier0Part::Clippy,
    Tier0Part::Build,
    Tier0Part::Test,
];

/// The hook `cargo xtask install-hook` writes. Cargo is looked for where rustup puts it too,
/// because git runs hooks without the shell's profile.
const HOOK: &str = "#!/bin/sh
# Written by `cargo xtask install-hook`: runs `cargo xtask ci prepush` before each push.
# `git push --no-verify` skips it.
PATH=\"$PATH:$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin\"
exec cargo xtask ci prepush --hook
";

/// `cargo xtask ci prepush [--list] [--hook]`.
pub(crate) fn prepush(args: &[String], profile: Profile) -> i32 {
    if args.iter().any(|a| a == "--list") {
        return crate::print_plan(&plan(profile));
    }
    if args.iter().any(|a| a == "--hook") {
        let mut input = String::new();
        if let Err(e) = std::io::stdin().read_to_string(&mut input) {
            eprintln!("prepush: could not read what git is pushing: {e}");
            return 1;
        }
        let pushed: Vec<String> = pushed_objects(&input)
            .into_iter()
            .map(|sha| commit_of(&sha).unwrap_or(sha))
            .collect();
        match hook_plan(&pushed, passed_commit().as_deref()) {
            HookPlan::Nothing => {
                println!("prepush: nothing to check (only deletions are pushed)");
                return 0;
            }
            HookPlan::AlreadyPassed(commit) => {
                println!("prepush: {commit} already passed `cargo xtask ci prepush`");
                return 0;
            }
            HookPlan::Run => {
                if let Some(head) = commit_of("HEAD") {
                    if !pushed.contains(&head) {
                        println!(
                            "prepush: checking the working tree at HEAD ({head}), which is not the \
                             commit pushed"
                        );
                    }
                }
            }
        }
    }
    let code = print_table("CI prepush -- before pushing", &reports(profile));
    if code == 0 && profile == Profile::TestRelease {
        record_pass();
    }
    code
}

/// The rows tier 0 has no part for, and the part each follows.
fn after(part: Tier0Part) -> Extra {
    match part {
        Tier0Part::Rules => Extra::Changelog,
        Tier0Part::Clippy => Extra::CrossClippy,
        _ => Extra::Nothing,
    }
}

/// A prepush row beyond tier 0's.
#[derive(Debug, PartialEq, Eq)]
enum Extra {
    Nothing,
    /// `cargo xtask changelog`: tier 0 runs in CI too, which has no `main` to compare with.
    Changelog,
    /// `cargo xtask cross-clippy`: CI runs clippy on Windows and Linux itself.
    CrossClippy,
}

fn reports(profile: Profile) -> Vec<Report> {
    let mut rows = Vec::new();
    for part in PARTS {
        rows.extend(crate::tier0_reports(profile, &[part]));
        match after(part) {
            Extra::Changelog => rows.push(changelog::report()),
            Extra::CrossClippy => rows.extend(cross_clippy::reports()),
            Extra::Nothing => {}
        }
    }
    rows
}

fn plan(profile: Profile) -> Vec<String> {
    let mut lines = Vec::new();
    for part in PARTS {
        lines.extend(crate::tier0_plan(profile, &[part]));
        match after(part) {
            Extra::Changelog => lines.push("xtask changelog".to_owned()),
            Extra::CrossClippy => lines.extend(cross_clippy::plan()),
            Extra::Nothing => {}
        }
    }
    lines
}

/// What the hook does for a push of `pushed` (the commits it sends) when `passed` is the commit
/// prepush last passed on.
#[derive(Debug, PartialEq, Eq)]
enum HookPlan {
    /// Only deletions are pushed.
    Nothing,
    /// Every commit pushed is the one that passed.
    AlreadyPassed(String),
    Run,
}

fn hook_plan(pushed: &[String], passed: Option<&str>) -> HookPlan {
    match (pushed.first(), passed) {
        (None, _) => HookPlan::Nothing,
        (Some(first), Some(passed)) if pushed.iter().all(|c| c == passed) => {
            HookPlan::AlreadyPassed(first.clone())
        }
        _ => HookPlan::Run,
    }
}

/// The objects a push sends, from the lines git gives a pre-push hook
/// (`<local ref> <local object> <remote ref> <remote object>`); a deletion sends none.
fn pushed_objects(input: &str) -> Vec<String> {
    let mut out: Vec<String> = input
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .filter(|sha| !sha.bytes().all(|b| b == b'0'))
        .map(str::to_owned)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The commit `rev` names (a tag object's commit for an annotated tag).
fn commit_of(rev: &str) -> Option<String> {
    git(&[
        "rev-parse",
        "--verify",
        "--quiet",
        &format!("{rev}^{{commit}}"),
    ])
}

/// `git <args>` in the workspace: its trimmed output when it succeeds.
fn git(args: &[&str]) -> Option<String> {
    Command::new("git")
        .args(args)
        .current_dir(workspace_root())
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Where the commit prepush last passed on is kept.
fn pass_file() -> PathBuf {
    target_dir().join("xtask").join("prepush-passed")
}

fn passed_commit() -> Option<String> {
    let text = std::fs::read_to_string(pass_file()).ok()?;
    Some(text.trim().to_owned()).filter(|c| !c.is_empty())
}

/// Records HEAD as passed when the tracked files are as committed; otherwise forgets any pass.
fn record_pass() {
    let file = pass_file();
    let clean =
        git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| s.is_empty());
    match commit_of("HEAD").filter(|_| clean) {
        Some(head) => {
            let written = file
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| std::fs::write(&file, format!("{head}\n")));
            if let Err(e) = written {
                eprintln!(
                    "prepush: could not record the pass in {}: {e}",
                    file.display()
                );
            }
        }
        None => {
            let _ = std::fs::remove_file(&file);
        }
    }
}

/// `cargo xtask install-hook [--force]`: writes [`HOOK`] as the repository's pre-push hook.
pub(crate) fn install_hook(args: &[String]) -> i32 {
    let force = args.iter().any(|a| a == "--force");
    let Some(hooks) = git(&["rev-parse", "--git-path", "hooks"]) else {
        eprintln!("install-hook: git could not say where this repository's hooks are");
        return 1;
    };
    let hook = workspace_root().join(hooks).join("pre-push");
    match write_hook(&hook, force) {
        Ok(()) => {
            println!("installed {}", hook.display());
            0
        }
        Err(e) => {
            eprintln!("install-hook: {e}");
            1
        }
    }
}

fn write_hook(hook: &Path, force: bool) -> Result<(), String> {
    match std::fs::read_to_string(hook) {
        Ok(text) if text == HOOK => return Ok(()),
        Ok(_) if !force => {
            return Err(format!(
                "{} is another hook; --force replaces it",
                hook.display()
            ))
        }
        _ => {}
    }
    if let Some(dir) = hook.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(hook, HOOK).map_err(|e| format!("{}: {e}", hook.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(hook, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("{}: {e}", hook.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "1111111111111111111111111111111111111111";
    const B: &str = "2222222222222222222222222222222222222222";
    const ZERO: &str = "0000000000000000000000000000000000000000";

    /// **Prepush runs every part of tier 0, cheapest first**, so a push is checked as fully as
    /// tier 0 checks and a broken rule or doc link fails it in seconds.
    #[test]
    fn prepush_runs_every_part_of_tier0_cheapest_first() {
        let mut sorted = PARTS.to_vec();
        sorted.sort_by_key(|p| Tier0Part::ALL.iter().position(|q| q == p));
        assert_eq!(sorted, Tier0Part::ALL);
        assert_eq!(PARTS[..2], [Tier0Part::Rules, Tier0Part::Docs]);
        assert_eq!(PARTS.last(), Some(&Tier0Part::Test));
    }

    /// **Prepush checks the changelog with the rules and clippy for the other systems after the
    /// Mac's**, so each fails as early as the checks like it.
    #[test]
    fn prepush_adds_the_changelog_and_cross_clippy() {
        let order: Vec<Extra> = PARTS.into_iter().map(after).collect();
        assert_eq!(order[0], Extra::Changelog);
        assert_eq!(PARTS[0], Tier0Part::Rules);
        let clippy = PARTS.iter().position(|p| *p == Tier0Part::Clippy).unwrap();
        assert_eq!(order[clippy], Extra::CrossClippy);
        let plan = plan(Profile::TestRelease);
        let at = |needle: &str| plan.iter().position(|l| l.contains(needle)).unwrap();
        assert!(at("xtask changelog") < at("xtask doc"));
        assert!(at("--features retail-dats --target x86_64-") > at("cargo clippy --workspace"));
    }

    /// **The hook reads what git pushes**: one object per ref, a deletion none, each once.
    #[test]
    fn the_hook_reads_the_objects_pushed_and_skips_deletions() {
        let input = format!(
            "refs/heads/main {A} refs/heads/main {B}\n\
             refs/tags/v0.2.0 {A} refs/tags/v0.2.0 {ZERO}\n\
             (delete) {ZERO} refs/tags/v0.1.0 {B}\n"
        );
        assert_eq!(pushed_objects(&input), [A]);
        assert!(pushed_objects(&format!("(delete) {ZERO} refs/tags/v0.1.0 {B}\n")).is_empty());
        assert!(pushed_objects("").is_empty());
    }

    /// **The hook runs the checks unless every commit pushed already passed them.**
    #[test]
    fn the_hook_skips_only_a_push_of_the_commit_that_passed() {
        let one = [A.to_owned()];
        let two = [A.to_owned(), B.to_owned()];
        assert_eq!(hook_plan(&[], Some(A)), HookPlan::Nothing);
        assert_eq!(
            hook_plan(&one, Some(A)),
            HookPlan::AlreadyPassed(A.to_owned())
        );
        assert_eq!(hook_plan(&one, None), HookPlan::Run);
        assert_eq!(hook_plan(&one, Some(B)), HookPlan::Run);
        assert_eq!(hook_plan(&two, Some(A)), HookPlan::Run);
    }

    /// **The hook runs prepush and nothing else**, from wherever git starts it.
    #[test]
    fn the_hook_runs_prepush() {
        assert!(HOOK.starts_with("#!/bin/sh\n"));
        assert!(HOOK.contains("\nexec cargo xtask ci prepush --hook\n"));
    }

    /// **install-hook leaves another hook alone** unless told to replace it, and rewriting its own
    /// hook changes nothing.
    #[test]
    fn install_hook_replaces_another_hook_only_when_forced() {
        let dir = std::env::temp_dir().join(format!("xtask-hook-{}", std::process::id()));
        let hook = dir.join("hooks").join("pre-push");
        write_hook(&hook, false).expect("a new hook");
        assert_eq!(std::fs::read_to_string(&hook).unwrap(), HOOK);
        write_hook(&hook, false).expect("the same hook again");
        std::fs::write(&hook, "#!/bin/sh\nexit 0\n").unwrap();
        assert!(write_hook(&hook, false).is_err());
        assert_eq!(
            std::fs::read_to_string(&hook).unwrap(),
            "#!/bin/sh\nexit 0\n"
        );
        write_hook(&hook, true).expect("forced");
        assert_eq!(std::fs::read_to_string(&hook).unwrap(), HOOK);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
