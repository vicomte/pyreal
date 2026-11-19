//! `UserPreferences.ini`.
//!
//! `UserPreferences` is a plain INI in the **install** directory with **one section per category** —
//! `[Sound]`, `[Render]`, `[Display]`, `[Camera]`, `[Input]`, `[Misc]`, `[UI]`, `[Net]`,
//! `[International]` — and the registry name is `sprintf("%s.%s", section, key)` on the way in,
//! which the save splits back apart at the name's **last** dot.

use super::PersistError;

/// The file name the default-file search builds, and that the client seeds its preferences-file
/// field with.
pub const FILE_NAME: &str = "UserPreferences.ini";

/// The section a name with **no** `.` in it falls back to.
///
/// It is *not* "the one section the client writes"; a parser that assumes so loses the section.
/// The save splits every registered name at its **last**
/// dot and writes `WritePrivateProfileStringA(<before>, <after>, value, path)`, so a real file is
/// `[Sound]` / `SoundVolume=…`, `[Render]` / `TextureFiltering=…`, one section per category. All 43
/// registered preferences contain a dot, so in practice `Default` never appears in a file the
/// client wrote.
pub const SECTION: &str = "Default";

/// The parsed `.ini`.
///
/// Entries keep their file order so a re-save is byte-identical: an INI reader that sorts its keys
/// cannot round-trip a file it did not write.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserPreferences {
    /// `(key, value)` in file order, keys **qualified** as `Category.Name`.
    ///
    /// The load's fifth step builds the name it pushes into the global registry with
    /// `sprintf("%s.%s", section, key)` — one format string, used by the load and by
    /// nothing else in retail. The bare
    /// key as written in the file is *not* what the registry is keyed by, and a reader that pushes
    /// it is looking `TextureFiltering` up in a registry that holds `Render.TextureFiltering`: it
    /// matches nothing at all.
    pub entries: Vec<(String, String)>,
    /// Lines that are not a `key=value` pair — blanks, comments **and section headers** — recorded
    /// with the index of the entry they precede so they survive a round trip in their original
    /// order. Headers live here rather than in a field of their own precisely so that a blank line
    /// written before a `[Section]` cannot be re-emitted after it.
    pub interleaved: Vec<(usize, String)>,
    /// The section *name* each header opened, with the index of the first entry under it. Used only
    /// by [`Self::to_text`], to take the prefix back off a qualified key.
    pub sections: Vec<(usize, String)>,
    /// Whether the file used CRLF. `fopen(path, "w")` is text mode on Windows, so a file the client
    /// wrote has CRLF.
    pub crlf: bool,
    /// Whether the file was Windows-1252 rather than UTF-8. Retail writes it that way, and the
    /// save writes it back the same way.
    pub ansi: bool,
}

impl UserPreferences {
    /// Look a **qualified** `Category.Name` up, ignoring case. A key the file has twice reads its
    /// last copy, as the registry load does.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    /// Set a **qualified** key's last copy. A key that is not already present is appended, which
    /// puts it under whatever section was last opened; rewriting the file properly is the client's
    /// job and is not this type's.
    pub fn set(&mut self, key: &str, value: &str) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .rev()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
        {
            e.1 = value.to_string();
        } else {
            self.entries.push((key.to_string(), value.to_string()));
        }
    }

    /// [`Self::parse`] over the file's bytes, decoded as [`text_of`] does. A UTF-16 file, or one
    /// with a NUL byte, is an error: neither decoding reads it, and a save would ruin it.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PersistError> {
        if bytes.starts_with(&[0xFF, 0xFE])
            || bytes.starts_with(&[0xFE, 0xFF])
            || bytes.contains(&0)
        {
            return Err(PersistError(
                "the preferences file is not 8-bit text".into(),
            ));
        }
        let (text, ansi) = text_of(bytes);
        Ok(Self {
            ansi,
            ..Self::parse(&text)?
        })
    }

    /// [`Self::to_text`] in the file's own encoding; a character with no 1252 byte saves as `?`.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        if self.ansi {
            pyreal_primitives::text::cp1252::encode_lossy(&self.to_text())
        } else {
            self.to_text().into_bytes()
        }
    }

    /// The load's steps 3-5.
    ///
    /// Retail enumerates section names (`GetPrivateProfileSectionNamesA`), then reads each section
    /// (`GetPrivateProfileSectionA`) and qualifies every key with its section. This is that, done
    /// in one pass over the text: for a file written by `WritePrivateProfileStringA`, where a
    /// section's keys are contiguous, it visits the same keys and builds the same names.
    ///
    /// A key seen **before** any `[Section]` header keeps its bare name. Retail cannot reach that
    /// state at all — `GetPrivateProfileSection` only ever hands out keys that are under a header —
    /// so the choice is free, and keeping the bare name leaves the two other readers in this
    /// workspace (`pyreal_audio::Prefs::from_ini` and `pyreal_client_runtime::config::Preferences::parse`, both
    /// of which default the section to `Default`) as the *more* forgiving ones rather than this.
    pub fn parse(text: &str) -> Result<Self, PersistError> {
        let crlf = text.contains("\r\n");
        let mut out = Self {
            crlf,
            ..Self::default()
        };
        let mut section = String::new();
        for raw in text.split('\n') {
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            let t = line.trim_start_matches('\u{feff}').trim();
            if t.len() >= 2 && t.starts_with('[') && t.ends_with(']') {
                section = t[1..t.len() - 1].to_string();
                out.sections.push((out.entries.len(), section.clone()));
                out.interleaved.push((out.entries.len(), line.to_string()));
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                if !k.trim().is_empty() {
                    let key = if section.is_empty() {
                        k.to_string()
                    } else {
                        format!("{section}.{k}")
                    };
                    out.entries.push((key, v.to_string()));
                    continue;
                }
            }
            out.interleaved.push((out.entries.len(), line.to_string()));
        }
        // A trailing newline leaves one empty interleaved line; drop it so the writer's own
        // terminator does not duplicate it.
        if out
            .interleaved
            .last()
            .is_some_and(|(i, l)| *i == out.entries.len() && l.is_empty())
        {
            out.interleaved.pop();
        }
        Ok(out)
    }

    /// The section in force at `entries[i]` — the last header at or before it.
    fn section_at(&self, i: usize) -> Option<&str> {
        self.sections
            .iter()
            .rev()
            .find(|(at, _)| *at <= i)
            .map(|(_, s)| s.as_str())
    }

    /// The key as it appears **in the file**: the qualified name with its section prefix taken back
    /// off, which is the save's split at the last dot run in reverse.
    fn bare_key<'a>(&self, i: usize, key: &'a str) -> &'a str {
        match self.section_at(i) {
            Some(s) if !s.is_empty() => key
                .get(..s.len())
                .filter(|head| head.eq_ignore_ascii_case(s))
                .and_then(|_| key[s.len()..].strip_prefix('.'))
                .unwrap_or(key),
            _ => key,
        }
    }

    /// The index one past the last entry that belongs to `section`, or the header's own index (and
    /// its place in `sections`) when the section exists and holds nothing. `None` when the file has
    /// no such section.
    fn section_end(&self, section: &str) -> Option<(usize, Option<usize>)> {
        let mut end = None;
        for i in 0..self.entries.len() {
            if self
                .section_at(i)
                .is_some_and(|s| s.eq_ignore_ascii_case(section))
            {
                end = Some((i + 1, None));
            }
        }
        end.or_else(|| {
            self.sections
                .iter()
                .position(|(_, s)| s.eq_ignore_ascii_case(section))
                .map(|h| (self.sections[h].0, Some(h)))
        })
    }

    /// `WritePrivateProfileStringA(section, key, value, path)` — the one call the save writes
    /// every preference with, and Win32's **merge**
    /// semantics rather than a rewrite.
    ///
    /// Three behaviours, and all three matter to a file the client did not write alone:
    ///
    /// 1. an existing key is replaced **in place**, so every other line keeps its position (its
    ///    last copy, the one the load reads);
    /// 2. a new key is appended to the **end of its own section**, not to the end of the file —
    ///    which is what keeps `[Sound]`'s keys contiguous, and therefore what keeps
    ///    `GetPrivateProfileSectionA` (and [`Self::parse`], which is that in one pass) able to
    ///    read them back under the right section name;
    /// 3. a section that is not in the file yet gets a `[Section]` header appended first.
    ///
    /// Keys the registry does not know — `Net.*`, `Input.KeymapFile`, anything a later client
    /// version added — are never visited and therefore survive untouched. That is the whole reason
    /// this is a merge: the save walks the *registry*, not the file, and a rewrite would silently
    /// drop every preference this build has not transcribed.
    ///
    /// Returns whether the file changed.
    pub fn write_profile_string(&mut self, section: &str, key: &str, value: &str) -> bool {
        let qualified = Self::key(section, key);
        if let Some(e) = self
            .entries
            .iter_mut()
            .rev()
            .find(|(k, _)| k.eq_ignore_ascii_case(&qualified))
        {
            let changed = e.1 != value;
            e.1 = value.to_string();
            return changed;
        }
        let (at, header, new_section) = match self.section_end(section) {
            Some((at, header)) => (at, header, false),
            None => (self.entries.len(), None, true),
        };
        // Everything that was anchored at or after `at` moves one along, so that a `[Render]`
        // header that sat in front of `entries[at]` before the insert still sits in front of it.
        // An empty section's own header, and what came before it, stay in front of the new key.
        let kept_sections = header.map_or(0, |h| h + 1);
        let kept_lines = header.map_or(0, |h| {
            let nth = self.sections[..h].iter().filter(|(a, _)| *a == at).count();
            self.interleaved
                .iter()
                .enumerate()
                .filter(|(_, (a, l))| *a == at && is_header(l))
                .nth(nth)
                .map_or(0, |(j, _)| j + 1)
        });
        for (j, (i, _)) in self.interleaved.iter_mut().enumerate() {
            if *i > at || (*i == at && j >= kept_lines) {
                *i += 1;
            }
        }
        for (j, (i, _)) in self.sections.iter_mut().enumerate() {
            if *i > at || (*i == at && j >= kept_sections) {
                *i += 1;
            }
        }
        if new_section {
            self.sections.push((at, section.to_string()));
            self.interleaved.push((at, format!("[{section}]")));
        }
        self.entries.insert(at, (qualified, value.to_string()));
        true
    }

    /// The save, reproducing the input byte for byte when nothing changed.
    #[must_use]
    pub fn to_text(&self) -> String {
        let nl = if self.crlf { "\r\n" } else { "\n" };
        let mut lines: Vec<String> = Vec::new();
        for (i, (k, v)) in self.entries.iter().enumerate() {
            for (at, l) in &self.interleaved {
                if *at == i {
                    lines.push(l.clone());
                }
            }
            lines.push(format!("{}={v}", self.bare_key(i, k)));
        }
        for (at, l) in &self.interleaved {
            if *at == self.entries.len() {
                lines.push(l.clone());
            }
        }
        let mut s = lines.join(nl);
        s.push_str(nl);
        s
    }

    /// The key format, `"%s.%s"` = `Category.Name`.
    #[must_use]
    pub fn key(category: &str, name: &str) -> String {
        format!("{category}.{name}")
    }
}

/// A preferences file's text: UTF-8 when it is, else Windows-1252, which is what retail writes.
/// The flag says which.
#[must_use]
pub fn text_of(bytes: &[u8]) -> (String, bool) {
    match std::str::from_utf8(bytes) {
        Ok(text) => (text.to_string(), false),
        Err(_) => (pyreal_primitives::text::cp1252::decode(bytes), true),
    }
}

/// The UI's own registered preferences.
pub mod keys {
    /// Bound to the UI element manager's tooltip delay.
    pub const TOOLTIP_DELAY: &str = crate::options::names::TOOLTIP_DELAY;
    /// Bound to the UI element manager's tooltip enable flag.
    pub const TOOLTIP_ENABLE: &str = crate::options::names::TOOLTIP_ENABLE;
    /// Bound to the process-wide IME preference.
    pub const USE_IME: &str = crate::options::names::USE_IME;
    pub const KEYMAP_FILE: &str = crate::options::names::KEYMAP_FILE;
    pub const CHAT_FONT_FACE: &str = crate::options::names::CHAT_FONT_FACE;
    pub const CHAT_FONT_SIZE: &str = crate::options::names::CHAT_FONT_SIZE;
}

/// Whether a kept line is a `[Section]` header, as [`UserPreferences::parse`] reads one.
fn is_header(line: &str) -> bool {
    let t = line.trim_start_matches('\u{feff}').trim();
    t.len() >= 2 && t.starts_with('[') && t.ends_with(']')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the load's step 5 (`sprintf("%s.%s", section, key)`) and the save's step 2
    /// (split at the **last** dot, section before, key after), which together make a
    /// **byte-identical** round trip. The fixture has the shape a file the client wrote actually
    /// has: one section per category.
    #[test]
    fn the_ini_round_trips_byte_identically() {
        let text = "[Misc]\r\nTooltipDelay=0.50\r\nTooltipEnable=1\r\n\
                    [International]\r\nUseIME=0\r\n[UI]\r\nChatFontSize=Medium\r\n";
        let p = UserPreferences::parse(text).unwrap();
        assert_eq!(p.to_text(), text, "byte-identical, headers and all");
        // The key the registry is keyed by is the qualified one — never the bare `TooltipDelay`.
        assert_eq!(p.get(keys::TOOLTIP_DELAY), Some("0.50"));
        assert_eq!(
            p.get("TooltipDelay"),
            None,
            "the bare key is not a registry name"
        );
        assert_eq!(p.get(keys::TOOLTIP_ENABLE), Some("1"));
        assert_eq!(p.get(keys::USE_IME), Some("0"));
        assert_eq!(p.get(keys::CHAT_FONT_SIZE), Some("Medium"));
        assert_eq!(p.get("Nope.Missing"), None);
        assert_eq!(p.sections.len(), 3);
        assert!(p.crlf);
    }

    /// The same, with LF and with comment and blank lines interleaved **between** sections: an INI
    /// reader that sorts, drops what it does not understand, or re-emits a header ahead of the
    /// blank line that preceded it cannot round-trip a file it did not write.
    #[test]
    fn comments_blank_lines_section_headers_and_key_order_all_survive() {
        let text = "[Sound]\n; a comment\nSoundVolume=1.00\n\n[Render]\nFieldOfView=90.00\n";
        let p = UserPreferences::parse(text).unwrap();
        assert_eq!(p.entries.len(), 2);
        assert_eq!(
            p.entries[0].0, "Sound.SoundVolume",
            "qualified, and in file order"
        );
        assert_eq!(p.entries[1].0, "Render.FieldOfView");
        assert_eq!(p.to_text(), text, "the blank line stays ahead of [Render]");
        assert!(!p.crlf);
    }

    /// A key ahead of any header keeps its bare name, and a `[Default]` header qualifies with
    /// `Default.` like any other — `Load`'s `"%s.%s"` has no special case for it, which is why a
    /// hand-written `[Default]` file full of already-dotted names matches **nothing** in the
    /// registry. Stated here because that was the shape three of this workspace's own fixtures had.
    #[test]
    fn the_default_section_is_not_special_cased() {
        let p = UserPreferences::parse("[Default]\r\nSound.SoundVolume=1.00\r\n").unwrap();
        assert_eq!(p.entries[0].0, "Default.Sound.SoundVolume");
        assert_eq!(p.to_text(), "[Default]\r\nSound.SoundVolume=1.00\r\n");
        let p = UserPreferences::parse("Stray=1\n[Sound]\nSoundVolume=1.00\n").unwrap();
        assert_eq!(p.entries[0].0, "Stray");
        assert_eq!(p.entries[1].0, "Sound.SoundVolume");
        assert_eq!(p.to_text(), "Stray=1\n[Sound]\nSoundVolume=1.00\n");
    }

    /// The key builder uses the `"%s.%s"` format of §3.1, and the six keys the UI
    /// registers are named there.
    #[test]
    fn the_registered_keys_are_the_documented_ones() {
        assert_eq!(
            UserPreferences::key("Misc", "TooltipDelay"),
            keys::TOOLTIP_DELAY
        );
        assert_eq!(keys::TOOLTIP_ENABLE, "Misc.TooltipEnable");
        assert_eq!(keys::USE_IME, "International.UseIME");
        assert_eq!(keys::KEYMAP_FILE, "Input.KeymapFile");
        assert_eq!(keys::CHAT_FONT_FACE, "UI.ChatFontFace");
        assert_eq!(keys::CHAT_FONT_SIZE, "UI.ChatFontSize");
    }

    /// Oracle: §3.2 — "`dataType`: 2 = enum, 3 = float, 4 = bool",
    /// No shipped call site registers a preference of type 1 or 5, so those are refused.
    #[test]
    fn write_profile_string_merges_rather_than_rewriting() {
        let text = "[Sound]\r\nSoundVolume=1.00\r\n[Camera]\r\nStiffness=0.45\r\n";
        let mut p = UserPreferences::parse(text).unwrap();

        // 1. in place, and the file is otherwise byte-identical.
        assert!(p.write_profile_string("Sound", "SoundVolume", "0.30"));
        assert!(
            !p.write_profile_string("Sound", "SoundVolume", "0.30"),
            "no change is no change"
        );
        assert_eq!(
            p.to_text(),
            "[Sound]\r\nSoundVolume=0.30\r\n[Camera]\r\nStiffness=0.45\r\n"
        );

        // 2. a new key lands at the end of **its own** section, ahead of the next header.
        assert!(p.write_profile_string("Sound", "SoundFeatures", "Mono"));
        assert_eq!(
            p.to_text(),
            "[Sound]\r\nSoundVolume=0.30\r\nSoundFeatures=Mono\r\n[Camera]\r\nStiffness=0.45\r\n"
        );
        // …and it is readable back under the right name, which is the point of putting it there.
        let back = UserPreferences::parse(&p.to_text()).unwrap();
        assert_eq!(back.get("Sound.SoundFeatures"), Some("Mono"));
        assert_eq!(back.get("Camera.SoundFeatures"), None);

        // 3. a section that is not in the file yet gets its header first.
        assert!(p.write_profile_string("UI", "ChatFontSize", "Large"));
        assert!(p
            .to_text()
            .ends_with("[Camera]\r\nStiffness=0.45\r\n[UI]\r\nChatFontSize=Large\r\n"));
        let back = UserPreferences::parse(&p.to_text()).unwrap();
        assert_eq!(back.get("UI.ChatFontSize"), Some("Large"));
        assert_eq!(back.entries.len(), 4);

        // A key nobody wrote is never touched: `Save` walks the registry, not the file.
        assert_eq!(back.get("Camera.Stiffness"), Some("0.45"));
    }

    #[test]
    fn a_new_key_in_an_empty_section_reads_back_under_that_section() {
        let mut p = UserPreferences::parse("[Sound]\nVol=1\n[Input]\n[Other]\nX=2\n").unwrap();
        assert!(p.write_profile_string("Input", "KeymapFile", "k"));
        let text = p.to_text();
        assert_eq!(
            text,
            "[Sound]\nVol=1\n[Input]\nKeymapFile=k\n[Other]\nX=2\n"
        );
        let q = UserPreferences::parse(&text).unwrap();
        assert_eq!(q.get("Input.KeymapFile"), Some("k"));
        assert_eq!(q.get("Other.X"), Some("2"));
    }

    #[test]
    fn a_new_key_under_a_lowercase_header_is_written_bare() {
        let mut p = UserPreferences::parse("[input]\nA=1\n").unwrap();
        assert!(p.write_profile_string("Input", "KeymapFile", "k"));
        assert_eq!(p.to_text(), "[input]\nA=1\nKeymapFile=k\n");
    }

    #[test]
    fn a_byte_order_mark_does_not_hide_the_first_section() {
        let p = UserPreferences::parse("\u{feff}[Input]\r\nKeymapFile=x\r\n").unwrap();
        assert_eq!(p.get("Input.KeymapFile"), Some("x"));
    }

    #[test]
    fn a_duplicated_key_reads_and_saves_its_last_copy() {
        let mut p = UserPreferences::parse("[Sound]\nSoundVolume=0.1\nSoundVolume=0.2\n").unwrap();
        assert_eq!(p.get("Sound.SoundVolume"), Some("0.2"), "load is last-wins");
        assert!(p.write_profile_string("Sound", "SoundVolume", "0.3"));
        let again = UserPreferences::parse(&p.to_text()).unwrap();
        assert_eq!(
            again.get("Sound.SoundVolume"),
            Some("0.3"),
            "the save changed a copy the next load does not read"
        );
    }

    #[test]
    fn a_windows_1252_file_reads_and_saves_back_byte_for_byte() {
        let bytes = b"[Input]\r\nKeymapFile=C:\\Users\\Jos\xe9\\acmap.txt\r\n";
        let mut p = UserPreferences::from_bytes(bytes).unwrap();
        assert_eq!(
            p.get("Input.KeymapFile"),
            Some("C:\\Users\\Jos\u{e9}\\acmap.txt")
        );
        assert_eq!(p.to_bytes(), bytes, "saved in the file's own encoding");
        p.write_profile_string("Sound", "SoundVolume", "0.5");
        assert!(p.to_bytes().starts_with(bytes));
        let utf8 = "[Input]\nKeymapFile=Jos\u{e9}\n";
        let q = UserPreferences::from_bytes(utf8.as_bytes()).unwrap();
        assert_eq!(q.get("Input.KeymapFile"), Some("Jos\u{e9}"));
        assert_eq!(q.to_bytes(), utf8.as_bytes(), "a UTF-8 file stays UTF-8");
        assert!(
            UserPreferences::from_bytes(&[0xFF, 0xFE, b'[', 0]).is_err(),
            "UTF-16"
        );
    }
}
