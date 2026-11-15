//! `pyreal-art fingerprint`: writes into a pack's manifest the fingerprint of the retail record
//! each model and material was made from, read from one set of the player's files
//! (`docs/ERAS.md`). The manifest's text is kept line for line; each fingerprint goes on the line
//! after the entry's `id` or `texture`.

use pyreal_assets::fingerprint::Fingerprint;
use pyreal_primitives::DataId;

/// What a run wrote.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Entries given a fingerprint.
    pub written: usize,
    /// Entries that named one already and were left as they were.
    pub kept: usize,
    /// Entries whose record the files do not hold, or will not read: the entry's first line.
    pub missing: Vec<String>,
}

/// The ids an entry's `id` or `texture` line names, in order.
fn ids_on(line: &str) -> Vec<DataId> {
    let Some((_, value)) = line.split_once('=') else {
        return Vec::new();
    };
    value
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter_map(|w| w.strip_prefix("0x").or_else(|| w.strip_prefix("0X")))
        .filter_map(|d| u32::from_str_radix(d, 16).ok())
        .map(DataId)
        .collect()
}

/// `text`, a manifest, with a fingerprint from `print` (a record's, as
/// `pyreal_art_pack::fingerprint::of_record` reads it) for each `[[model]]` and `[[material]]` that
/// names none; with `replace`, for every one, in place of what it names.
///
/// # Errors
/// The manifest written would not read.
pub fn fingerprint(
    text: &str,
    print: impl Fn(DataId) -> Option<Fingerprint>,
    replace: bool,
) -> Result<(String, Report), String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + lines.len() / 4);
    let mut report = Report::default();
    let mut i = 0;
    while i < lines.len() {
        let header = lines[i].trim();
        out.push(lines[i].to_owned());
        i += 1;
        let key = match header {
            "[[model]]" => "id",
            "[[material]]" => "texture",
            _ => continue,
        };
        // The entry's own keys run to the next table's header.
        let end = (i..lines.len())
            .find(|&k| lines[k].trim_start().starts_with('['))
            .unwrap_or(lines.len());
        let named = |k: usize, name: &str| {
            lines[k]
                .split_once('=')
                .is_some_and(|(n, _)| n.trim() == name)
        };
        let has_print = (i..end).any(|k| named(k, "fingerprint"));
        let at = (i..end).find(|&k| named(k, key));
        let Some(at) = at.filter(|_| replace || !has_print) else {
            if has_print {
                report.kept += 1;
            }
            out.extend(lines[i..end].iter().map(|l| (*l).to_owned()));
            i = end;
            continue;
        };
        let ids = ids_on(lines[at]);
        let prints: Option<Vec<String>> = ids
            .iter()
            .map(|&id| print(id).map(|f| format!("\"{f}\"")))
            .collect();
        for k in i..end {
            if !named(k, "fingerprint") || prints.is_none() {
                out.push(lines[k].to_owned());
            }
            if k == at {
                match &prints {
                    Some(p) if p.len() == 1 => out.push(format!("fingerprint = {}", p[0])),
                    Some(p) if !p.is_empty() => {
                        out.push(format!("fingerprint = [{}]", p.join(", ")));
                    }
                    _ => report.missing.push(lines[at].trim().to_owned()),
                }
            }
        }
        if prints.as_ref().is_some_and(|p| !p.is_empty()) {
            report.written += 1;
        }
        i = end;
    }
    let mut written = out.join("\n");
    if text.ends_with('\n') {
        written.push('\n');
    }
    pyreal_assets::art_pack::ArtPackManifest::parse(&written).map_err(|e| e.to_string())?;
    Ok((written, report))
}

/// Writes each picture `pictures` names from `store` into `out` as `<id>.png`, decoded as the
/// engine decodes it (with its first colours clear where its flag says it cuts out), and each
/// one's fingerprint into `out/fingerprints.tsv`, a line each; the ids it could not read.
///
/// # Errors
/// The folder or a file will not write.
pub fn export(
    store: &pyreal_dat::RetailDatStore,
    pictures: &[(DataId, bool)],
    out: &std::path::Path,
) -> Result<Vec<DataId>, String> {
    use pyreal_scene::art_pack::fingerprint::{of_record, pixels};
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut prints = String::new();
    let mut missing = Vec::new();
    for &(id, clip) in pictures {
        let (Some((w, h, bgra)), Some(print)) = (pixels(store, id, clip), of_record(store, id))
        else {
            missing.push(id);
            continue;
        };
        let rgba: Vec<u8> = bgra
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();
        let path = out.join(format!("0x{:08X}.png", id.0));
        let bytes =
            crate::pictures::png_of(w, h, &rgba).map_err(|e| format!("{}: {e}", path.display()))?;
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        prints += &format!("0x{:08X}\t{print}\n", id.0);
    }
    let path = out.join("fingerprints.tsv");
    std::fs::write(&path, prints).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(missing)
}

/// The pictures a list names, a line each: `0x` and the id, and `clip` where it cuts out; a
/// line starting `#` and a blank line are skipped.
#[must_use]
pub fn list(text: &str) -> Vec<(DataId, bool)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let mut words = l.split_whitespace();
            let id = words.next()?;
            let id = u32::from_str_radix(id.strip_prefix("0x").unwrap_or(id), 16).ok()?;
            Some((DataId(id), words.any(|w| w == "clip")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACK: &str = "[pack]\nname = \"x\"\nformat = 1\n\n\
[[model]]\nid = \"0x02000001\"\nauthor = \"a\"\ntool = \"t\"\nlicense = \"l\"\n\n\
[[model.lod]]\nfile = \"m.gltf\"\n\n\
[[material]]\ntexture = [\"0x05000001\", \"0x05000002\"]\nbase_color = \"b.png\"\n\
author = \"a\"\ntool = \"t\"\nlicense = \"l\"\n\n\
[[material]]\ntexture = \"0x05000003\"\nbase_color = \"b.png\"\n\
author = \"a\"\ntool = \"t\"\nlicense = \"l\"\n";

    fn print(id: DataId) -> Option<Fingerprint> {
        match id.0 {
            0x0200_0001 => Some(Fingerprint::Model(7)),
            0x0500_0003 => None,
            hash => Some(Fingerprint::Picture {
                width: 64,
                height: 64,
                hash: u64::from(hash & 0xFF),
            }),
        }
    }

    #[test]
    fn each_entry_gets_its_records_fingerprint_on_the_line_after_its_id() {
        let (text, report) = fingerprint(PACK, print, false).expect("reads");
        assert!(
            text.contains("id = \"0x02000001\"\nfingerprint = \"model 0000000000000007\"\nauthor")
        );
        assert!(text.contains(
            "fingerprint = [\"picture 64x64 0000000000000001\", \"picture 64x64 0000000000000002\"]"
        ));
        assert_eq!(report.written, 2);
        assert_eq!(report.missing, ["texture = \"0x05000003\""]);

        let (again, report) = fingerprint(&text, print, false).expect("reads");
        assert_eq!(again, text, "what names one is kept");
        assert_eq!(report.kept, 2);
        let (replaced, _) =
            fingerprint(&text, |_| Some(Fingerprint::Model(9)), true).expect("reads");
        assert_eq!(replaced.matches("fingerprint").count(), 3);
        assert!(replaced.contains("model 0000000000000009"));
    }

    #[test]
    fn a_list_names_each_picture_and_whether_it_cuts_out() {
        let wanted = list("# the 2005 pictures\n0x05000003\n\n0x0500000C clip\nnot an id\n");
        assert_eq!(
            wanted,
            [(DataId(0x0500_0003), false), (DataId(0x0500_000C), true)]
        );
    }
}
