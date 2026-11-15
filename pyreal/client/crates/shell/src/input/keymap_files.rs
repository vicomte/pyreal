//! Keymap files, named schemes and the separately stored interface maps.

use super::InputShell;
use std::path::PathBuf;

/// The keyboard UI's three non-error keymap-save outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveKeymapAs {
    Saved,
    NeedsOverwrite,
    ReadOnly,
}

/// Keymap initialization step 2: resolve where the `.keymap` file lives.
///
/// ```text
/// dir  = the user-preferences settings directory
/// name = Input.KeymapFile, or the running executable's file name with the extension -> "keymap"
/// path = (dir joined with name, true)
/// ```
///
/// `keymap_file` is the `Input.KeymapFile` preference, whose
/// help text is *"The filename of the keymap file to use"*. `None` or empty takes the default,
/// which is [`DEFAULT_KEYMAP_FILE`].
///
/// **The default is a constant, and retail's rule was not.** Retail's keymap initialization takes
/// the *running executable's* file name and swaps the extension for `keymap`, which is the only
/// reason retail's file is called `acclient.keymap`. A rule that reads the binary's name means renaming or copying the binary silently orphans the
/// player's bindings -- a `pyreal-debug.exe` or a second copy under any other name comes up
/// with no keymap and writes a new one beside the old. The file is the player's, not the
/// executable's, so it gets a fixed name.
///
/// Returns `None` when `preferences_file` is empty, which is an empty keymap-file path and the one
/// state in which keymap cleanup writes nothing.
#[must_use]
pub fn keymap_path_for(
    preferences_file: &std::path::Path,
    keymap_file: Option<&str>,
) -> Option<PathBuf> {
    if preferences_file.as_os_str().is_empty() {
        return None;
    }
    let dir = preferences_file
        .parent()
        .map_or_else(PathBuf::new, std::path::Path::to_path_buf);
    let name = match keymap_file.filter(|s| !s.is_empty()) {
        Some(n) => PathBuf::from(n),
        None => PathBuf::from(DEFAULT_KEYMAP_FILE),
    };
    Some(dir.join(name))
}

/// The `.keymap` file a run with no `Input.KeymapFile` preference reads and writes, in the
/// settings directory. Retail's equivalent is `acclient.keymap`; see [`keymap_path_for`] for why
/// this is a constant and that one is not.
pub const DEFAULT_KEYMAP_FILE: &str = "dereth-modern.keymap";

/// The modern (retail) interface's slug: its key maps are `<name>-modern.keymap`.
pub const MODERN_SLUG: &str = "modern";

/// The file of the key map `name` of the interface whose slug is `slug`: `<name>-<slug>.keymap`.
/// A name typed with the slug or the extension already on is taken without them.
#[must_use]
pub fn scheme_file(name: &str, slug: &str) -> String {
    let name = name.trim();
    let name = strip_suffix_ignore_case(name, ".keymap").unwrap_or(name);
    let tail = format!("-{slug}");
    let name = strip_suffix_ignore_case(name, &tail).unwrap_or(name);
    format!("{name}-{slug}.keymap")
}

/// The name of the key map in `file` when it is one of the interface whose slug is `slug`.
#[must_use]
pub fn scheme_name(file: &str, slug: &str) -> Option<String> {
    let tail = format!("-{slug}.keymap");
    strip_suffix_ignore_case(file, &tail)
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
}

fn strip_suffix_ignore_case<'a>(s: &'a str, suffix: &str) -> Option<&'a str> {
    let at = s.len().checked_sub(suffix.len())?;
    (s.is_char_boundary(at) && s[at..].eq_ignore_ascii_case(suffix)).then(|| &s[..at])
}

/// The input manager's full merged keymap, written where the host keeps the client's files.
///
/// # Errors
/// [`pyreal_input::InputError::Io`] if the file cannot be written.
fn write_keymap(
    map: &pyreal_input::MasterInputMap,
    path: &std::path::Path,
) -> Result<(), pyreal_input::InputError> {
    pyreal_client_runtime::platform::files::write(path, map.to_keymap_text())?;
    Ok(())
}

/// Persist `Input.KeymapFile` with `WritePrivateProfileString`-style merge semantics.
pub fn save_keymap_preference(
    preferences_file: &std::path::Path,
    keymap_file: &str,
) -> Result<(), std::io::Error> {
    if preferences_file.as_os_str().is_empty() {
        return Ok(());
    }
    let mut ini = pyreal_client_runtime::config::read_user_preferences(preferences_file)?
        .unwrap_or(
            pyreal_client_contract::persist::preferences::UserPreferences {
                crlf: true,
                ..Default::default()
            },
        );
    ini.write_profile_string("Input", "KeymapFile", keymap_file);
    pyreal_client_runtime::platform::files::write(preferences_file, ini.to_bytes())
}

/// Whether the key map `text` read from `path` does not parse, logged when it does not.
pub(super) fn unreadable(path: Option<&std::path::PathBuf>, text: &str) -> bool {
    let Err(error) = pyreal_input::MasterInputMap::from_keymap_text(text) else {
        return false;
    };
    tracing::error!(
        path = ?path,
        %error,
        "the key map file does not read: the default keys are used and the file is left as it is"
    );
    true
}

impl InputShell {
    /// The configured keymap filename resolved to a path, or `None` when the client would not save.
    #[must_use]
    pub fn keymap_path(&self) -> Option<&std::path::Path> {
        self.keymap_path.as_deref()
    }

    /// The keymap basename shown by the keyboard options panel.
    #[must_use]
    pub fn keymap_file_name(&self) -> Option<String> {
        self.keymap_path
            .as_deref()
            .and_then(std::path::Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
    }

    /// The keyboard page's label, without changing the file used for persistence.
    #[must_use]
    pub fn keymap_display_name(&self) -> Option<String> {
        let file = self
            .keymap_file_name()
            .unwrap_or_else(|| DEFAULT_KEYMAP_FILE.into());
        if file.eq_ignore_ascii_case(DEFAULT_KEYMAP_FILE)
            || file.eq_ignore_ascii_case("acclient.keymap")
        {
            Some("Default".into())
        } else {
            scheme_name(&file, MODERN_SLUG).or(Some(file))
        }
    }

    /// The names of one interface's saved key maps (`<name>-<slug>.keymap`) in the folder,
    /// sorted.
    pub fn scheme_names(&self, slug: &str) -> Result<Vec<String>, pyreal_input::InputError> {
        Ok(self
            .keymap_files()?
            .iter()
            .filter_map(|f| scheme_name(f, slug))
            .collect())
    }

    /// The name of the modern interface's key map in use, when its file is one of the scheme's.
    #[must_use]
    pub fn modern_scheme_in_use(&self) -> Option<String> {
        self.keymap_file_name()
            .and_then(|f| scheme_name(&f, MODERN_SLUG))
    }

    /// Every `*.keymap` in the folder, by basename.
    pub fn keymap_files(&self) -> Result<Vec<String>, pyreal_input::InputError> {
        let Some(dir) = self
            .keymap_path
            .as_deref()
            .and_then(std::path::Path::parent)
        else {
            return Ok(Vec::new());
        };
        let mut names = Vec::new();
        for path in pyreal_client_runtime::platform::files::list(dir)? {
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("keymap"))
            {
                if let Some(name) = path.file_name() {
                    names.push(name.to_string_lossy().into_owned());
                }
            }
        }
        names.sort_by_key(|name| name.to_ascii_lowercase());
        Ok(names)
    }

    /// Initialize the keymap from the selected file when the load dialog closes.
    /// A malformed user file deliberately becomes the two shipped defaults: keymap
    /// initialization clears the partial user map after adding it fails and then adds
    /// the game keymap and the base keymap, in that order.
    pub fn load_keymap_file(&mut self, name: &str) -> Result<bool, pyreal_input::InputError> {
        let Some(dir) = self
            .keymap_path
            .as_deref()
            .and_then(std::path::Path::parent)
        else {
            return Ok(false);
        };
        let Some(file_name) = std::path::Path::new(name).file_name() else {
            return Ok(false);
        };
        if file_name != std::path::Path::new(name) {
            return Ok(false);
        }
        let path = dir.join(file_name);
        let text = pyreal_client_runtime::platform::files::read_to_string(&path)?;
        let unreadable = unreadable(Some(&path), &text);
        let Some(maps) = self.manager.shipped_maps.take() else {
            return Err(pyreal_input::InputError::KeymapFile(
                "the shipped keymaps are unavailable".to_owned(),
            ));
        };
        let merged = pyreal_input::InputManager::load_over_defaults(
            Some(&text),
            &[&maps.0, &maps.1],
            Some(&self.manager.action_map),
        );
        *self.modern_map_mut() = merged;
        self.manager.shipped_maps = Some(maps);
        self.keymap_path = Some(path);
        self.keymap_unreadable = unreadable;
        Ok(true)
    }

    /// Back to the shipped defaults: the player's own keys are dropped.
    pub fn restore_shipped_keys(&mut self) -> bool {
        let Some((game, base)) = self.manager.shipped_maps.as_deref() else {
            return false;
        };
        let restored = pyreal_input::InputManager::load_over_defaults(
            None,
            &[game, base],
            Some(&self.manager.action_map),
        );
        *self.modern_map_mut() = restored;
        true
    }

    /// Delete the key map file `name` (a basename, `.keymap` added when absent) beside the one in
    /// use. The one in use is not deleted.
    pub fn delete_keymap_file(&mut self, name: &str) -> Result<bool, pyreal_input::InputError> {
        let Some(dir) = self
            .keymap_path
            .as_deref()
            .and_then(std::path::Path::parent)
        else {
            return Ok(false);
        };
        let mut file = std::path::PathBuf::from(name.trim());
        if file.file_name() != Some(file.as_os_str()) {
            return Ok(false);
        }
        if file
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("keymap"))
        {
            file = std::path::PathBuf::from(format!("{}.keymap", name.trim()));
        }
        let path = dir.join(file);
        if self.keymap_path.as_deref() == Some(path.as_path()) {
            return Ok(false);
        }
        pyreal_client_runtime::platform::files::remove_file(&path)?;
        Ok(true)
    }

    /// `SaveKeymap` for a new name from the Save Keymap dialog: the modern interface's key map
    /// `name`, the file `<name>-modern.keymap`.
    pub fn save_keymap_as(
        &mut self,
        name: &str,
        overwrite: bool,
    ) -> Result<Option<SaveKeymapAs>, pyreal_input::InputError> {
        let Some(dir) = self
            .keymap_path
            .as_deref()
            .and_then(std::path::Path::parent)
        else {
            return Ok(None);
        };
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }
        let file_name = std::path::PathBuf::from(scheme_file(name, MODERN_SLUG));
        if file_name.file_name().is_none() || file_name.file_name() != Some(file_name.as_os_str()) {
            return Ok(None);
        }
        let path = dir.join(file_name);
        if pyreal_client_runtime::platform::files::exists(&path) {
            if pyreal_client_runtime::platform::files::read_only(&path)? {
                return Ok(Some(SaveKeymapAs::ReadOnly));
            }
            if !overwrite {
                return Ok(Some(SaveKeymapAs::NeedsOverwrite));
            }
        }
        if let Err(error) = write_keymap(self.modern_map(), &path) {
            if matches!(&error, pyreal_input::InputError::Io(io) if io.kind() == std::io::ErrorKind::PermissionDenied)
            {
                return Ok(Some(SaveKeymapAs::ReadOnly));
            }
            return Err(error);
        }
        self.keymap_path = Some(path);
        self.keymap_unreadable = false;
        Ok(Some(SaveKeymapAs::Saved))
    }

    /// Save the keymap during cleanup when its filename is nonempty and it was loaded.
    /// The destination joins the settings directory and that filename.
    ///
    /// Saving does not require the preference `Input.KeymapFile` to be set. Initialization assigns
    /// the keymap-file path either from the preference or, when the preference is empty, from
    /// the running executable's file name with the extension changed to `keymap` — so
    /// the path is **never** empty after successful keymap initialization, and a retail client
    /// therefore writes `<prefs dir>\acclient.keymap` on every clean exit. That is why no shipped
    /// install carries a `.keymap` and every played one does.
    ///
    /// What is written is the **full merged map**, defaults included, not just the overrides
    /// — [`pyreal_input::InputManager::save_keymap`] is the writer and this is its caller.
    ///
    /// Returns `Ok(false)` when there is no path, which is the client's own skip, or when its file
    /// did not parse.
    ///
    /// # Errors
    /// [`pyreal_input::InputError::Io`] if the file cannot be written.
    pub fn save_keymap(&self) -> Result<bool, pyreal_input::InputError> {
        let Some(path) = self
            .keymap_path
            .as_ref()
            .filter(|_| !self.keymap_unreadable)
        else {
            return Ok(false);
        };
        write_keymap(self.modern_map(), path)?;
        Ok(true)
    }
}
