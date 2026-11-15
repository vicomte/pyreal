//! Where passwords live: the operating system's secret store, and nowhere else.
//!
//! Pyreal's own files never hold a password (see [`crate::state`]). The client keeps them in the
//! Keychain on macOS, Credential Manager on Windows and the Secret Service on Linux, through this
//! trait, where the player can see and delete them.

/// The entry name for one account on one world: `pyreal:world/<slug>/<username>`. The account name
/// is lower-cased, as the client does, so one account is one entry however it was typed.
pub fn target(world_slug: &str, username: &str) -> String {
    format!("{TARGET_PREFIX}{world_slug}/{}", username.to_lowercase())
}

/// The prefix every entry Pyreal writes starts with, for "Forget all passwords".
pub const TARGET_PREFIX: &str = "pyreal:world/";

/// The entry the old launcher kept the same account's password under:
/// `dereth:world/<slug>/<username>`. Read for a world imported from it ([`password`]), deleted
/// with Pyreal's ([`forget`]), never written.
pub fn legacy_target(world_slug: &str, username: &str) -> String {
    format!("dereth:world/{world_slug}/{}", username.to_lowercase())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultError(pub String);

impl core::fmt::Display for VaultError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for VaultError {}

/// A secret store.
pub trait Vault: core::fmt::Debug {
    fn get(&self, target: &str) -> Result<Option<String>, VaultError>;
    fn set(&mut self, target: &str, username: &str, secret: &str) -> Result<(), VaultError>;
    /// Deleting an entry that is not there is not an error.
    fn delete(&mut self, target: &str) -> Result<(), VaultError>;
    /// The targets under a prefix.
    fn list(&self, prefix: &str) -> Result<Vec<String>, VaultError>;

    /// Delete every entry this launcher wrote.
    fn forget_all(&mut self) -> Result<usize, VaultError> {
        let all = self.list(TARGET_PREFIX)?;
        for t in &all {
            self.delete(t)?;
        }
        Ok(all.len())
    }
}

/// A vault that forgets everything when dropped. For tests, and for a front end with no store at
/// all, where "remember" simply cannot be offered.
#[derive(Debug, Default, Clone)]
pub struct MemoryVault {
    entries: std::collections::BTreeMap<String, String>,
}

impl Vault for MemoryVault {
    fn get(&self, target: &str) -> Result<Option<String>, VaultError> {
        Ok(self.entries.get(target).cloned())
    }

    fn set(&mut self, target: &str, _username: &str, secret: &str) -> Result<(), VaultError> {
        self.entries.insert(target.to_owned(), secret.to_owned());
        Ok(())
    }

    fn delete(&mut self, target: &str) -> Result<(), VaultError> {
        self.entries.remove(target);
        Ok(())
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, VaultError> {
        Ok(self
            .entries
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}

/// The password for `username` on `world_slug`: Pyreal's entry, else, for a world `imported` from
/// the old launcher, its entry ([`legacy_target`]), which is then kept as Pyreal's so it is found
/// there next time. The old entry is left as it was.
///
/// # Errors
/// [`VaultError`] when the store cannot be read.
pub fn password(
    vault: &mut dyn Vault,
    world_slug: &str,
    username: &str,
    imported: bool,
) -> Result<Option<String>, VaultError> {
    let own = target(world_slug, username);
    if let Some(p) = vault.get(&own)? {
        return Ok(Some(p));
    }
    if !imported {
        return Ok(None);
    }
    let Some(p) = vault.get(&legacy_target(world_slug, username))? else {
        return Ok(None);
    };
    // Copied, not moved: a failure to keep it costs nothing but the next lookup.
    let _ = vault.set(&own, username, &p);
    Ok(Some(p))
}

/// Forget the password for `username` on `world_slug`: Pyreal's entry, and the old launcher's
/// for a world `imported` from it, so it does not come back.
///
/// # Errors
/// [`VaultError`] when an entry cannot be deleted.
pub fn forget(
    vault: &mut dyn Vault,
    world_slug: &str,
    username: &str,
    imported: bool,
) -> Result<(), VaultError> {
    vault.delete(&target(world_slug, username))?;
    if imported {
        vault.delete(&legacy_target(world_slug, username))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_account_is_one_entry_however_it_is_typed() {
        assert_eq!(target("eulmore", "Player"), "pyreal:world/eulmore/player");
        assert_eq!(target("eulmore", "PLAYER"), target("eulmore", "player"));
        assert_ne!(target("eulmore", "player"), target("achard", "player"));
    }

    #[test]
    fn forget_all_takes_only_our_entries() {
        let mut v = MemoryVault::default();
        v.set(&target("a", "x"), "x", "1").unwrap();
        v.set(&target("b", "y"), "y", "2").unwrap();
        v.set("someone-else", "z", "3").unwrap();
        assert_eq!(v.forget_all().unwrap(), 2);
        assert_eq!(v.get("someone-else").unwrap().as_deref(), Some("3"));
        v.delete("not-there").unwrap();
    }

    #[test]
    fn the_old_launchers_password_is_found_and_kept_as_pyreals() {
        let mut v = MemoryVault::default();
        v.set(&legacy_target("coldeve", "lark"), "lark", "old")
            .unwrap();
        assert_eq!(
            password(&mut v, "coldeve", "Lark", true)
                .unwrap()
                .as_deref(),
            Some("old")
        );
        assert_eq!(
            v.get(&target("coldeve", "lark")).unwrap().as_deref(),
            Some("old")
        );
        assert!(v.get(&legacy_target("coldeve", "lark")).unwrap().is_some());
        v.set(&target("coldeve", "lark"), "lark", "new").unwrap();
        assert_eq!(
            password(&mut v, "coldeve", "lark", true)
                .unwrap()
                .as_deref(),
            Some("new")
        );
        assert_eq!(password(&mut v, "coldeve", "nobody", true).unwrap(), None);
    }

    #[test]
    fn a_world_not_imported_never_takes_the_old_launchers_password() {
        let mut v = MemoryVault::default();
        v.set(&legacy_target("custom-1", "lark"), "lark", "old")
            .unwrap();
        assert_eq!(password(&mut v, "custom-1", "lark", false).unwrap(), None);
        assert_eq!(v.get(&target("custom-1", "lark")).unwrap(), None);
        forget(&mut v, "custom-1", "lark", false).unwrap();
        assert!(v.get(&legacy_target("custom-1", "lark")).unwrap().is_some());
        forget(&mut v, "custom-1", "lark", true).unwrap();
        assert_eq!(v.get(&legacy_target("custom-1", "lark")).unwrap(), None);
    }
}
