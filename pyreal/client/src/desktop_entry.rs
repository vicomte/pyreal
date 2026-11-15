//! The Linux desktop's entry for Pyreal, and the icon it names.
//!
//! A Linux desktop finds a window's icon and name through a `.desktop` file whose name, or whose
//! `StartupWMClass`, matches the window's application id (on Wayland) or its window class (on
//! X11). An AppImage or a tarball installs nothing, so the client writes two files under the
//! user's data folder (`$XDG_DATA_HOME`, by default `~/.local/share`), each only when its content
//! differs: `applications/<id>.desktop`, which also opens `pyreal://` join links, and
//! `icons/hicolor/256x256/apps/<id>.png`.

use std::path::{Path, PathBuf};

/// The MIME type a Linux desktop opens the join links' scheme by.
pub const LINUX_MIME: &str = "x-scheme-handler/pyreal";

/// A path quoted as a `.desktop` file's `Exec` key takes one argument: in double quotes, with the
/// characters the quoting reserves escaped, then the string escapes applied, and `%` doubled so it
/// is not read as a field code.
fn exec_argument(path: &Path) -> String {
    let mut quoted = String::from("\"");
    for c in path.display().to_string().chars() {
        match c {
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(c);
            }
            '\\' => quoted.push_str("\\\\\\\\"),
            '%' => quoted.push_str("%%"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// The entry, starting `exec` with a join link as its argument, for the application `app_id`.
#[must_use]
pub fn entry(exec: &Path, app_id: &str) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=Pyreal\nComment=Asheron's Call\n\
         Exec={} %u\nIcon={app_id}\nTerminal=false\nCategories=Game;\nMimeType={LINUX_MIME};\n\
         StartupWMClass={app_id}\n",
        exec_argument(exec)
    )
}

/// The user's data folder: `$XDG_DATA_HOME` when it is an absolute path, else `~/.local/share`;
/// `None` when neither is known.
pub fn data_home(var: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    if let Some(dir) = var("XDG_DATA_HOME").map(PathBuf::from) {
        if dir.is_absolute() {
            return Some(dir);
        }
    }
    let home = var("HOME").map(PathBuf::from).filter(|h| h.is_absolute())?;
    Some(home.join(".local/share"))
}

/// What the entry starts: the AppImage this process runs from (`$APPIMAGE`), else `this_program`.
/// The program inside an AppImage is at a different path on every start.
pub fn entry_target(
    var: &dyn Fn(&str) -> Option<std::ffi::OsString>,
    this_program: PathBuf,
) -> PathBuf {
    var("APPIMAGE")
        .filter(|v| !v.is_empty())
        .map_or(this_program, PathBuf::from)
}

/// The two files, as (path, content), under `data_home`: the entry, starting `exec`, and its icon.
#[must_use]
pub fn files(data_home: &Path, exec: &Path, app_id: &str, icon: &[u8]) -> Vec<(PathBuf, Vec<u8>)> {
    vec![
        (
            data_home
                .join("applications")
                .join(format!("{app_id}.desktop")),
            entry(exec, app_id).into_bytes(),
        ),
        (
            data_home
                .join("icons/hicolor/256x256/apps")
                .join(format!("{app_id}.png")),
            icon.to_vec(),
        ),
    ]
}

/// Make sure each of `files` is on disk with its content, writing only the ones whose content
/// differs (or that are missing). Returns the files written.
///
/// # Errors
/// A folder or file could not be written.
pub fn write_changed(files: &[(PathBuf, Vec<u8>)]) -> std::io::Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    for (path, bytes) in files {
        if std::fs::read(path).is_ok_and(|on_disk| on_disk == *bytes) {
            continue;
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, bytes)?;
        written.push(path.clone());
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_opens_join_links_and_is_written_once() {
        let dir = std::env::temp_dir().join(format!("pyreal-desktop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let text = entry(Path::new("/opt/Pyreal 100%/pyreal"), "dev.pyreal.client");
        assert!(
            text.contains("Exec=\"/opt/Pyreal 100%%/pyreal\" %u\n"),
            "{text}"
        );
        assert!(text.contains("MimeType=x-scheme-handler/pyreal;\n"));
        let f = files(&dir, Path::new("/x"), "dev.pyreal.client", b"png");
        assert_eq!(write_changed(&f).unwrap().len(), 2);
        assert!(write_changed(&f).unwrap().is_empty(), "nothing changed");
        let var = |k: &str| (k == "HOME").then(|| std::ffi::OsString::from("/home/a"));
        // `/home/a` is not an absolute path on Windows, where these tests also run.
        if cfg!(unix) {
            assert_eq!(data_home(&var), Some(PathBuf::from("/home/a/.local/share")));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
