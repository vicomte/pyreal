//! `NOTICE.txt` and `THIRD-PARTY-LICENSES.html`: what a release says about its source, its
//! origins and everything linked into it.

/// One crate the binaries are built from, as `cargo tree` names it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Crate {
    pub name: String,
    pub version: String,
    pub licence: String,
}

/// The crates in `cargo tree --prefix none --format "{p}|{l}"` output, each once, sorted.
/// A line reads `name vX.Y.Z [(path)] [(proc-macro)] [(*)]|licence`.
pub fn parse_cargo_tree(output: &str) -> Vec<Crate> {
    let mut crates: Vec<Crate> = output
        .lines()
        .filter_map(|line| {
            let (package, licence) = line.split_once('|')?;
            let mut words = package.split_whitespace();
            let name = words.next()?.to_owned();
            let version = words.next()?.strip_prefix('v')?.to_owned();
            let licence = licence.trim().trim_end_matches("(*)").trim().to_owned();
            Some(Crate {
                name,
                version,
                licence,
            })
        })
        .collect();
    crates.sort();
    crates.dedup();
    crates
}

/// Whether a crate is one of this repository's own (the `pyreal-*` crates and the launcher's
/// `auberean-friends`, all MIT).
pub fn is_own(c: &Crate) -> bool {
    c.name.starts_with("pyreal-") || c.name == "auberean-friends"
}

fn indent(text: &str) -> String {
    let mut out: String = text
        .lines()
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("    {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    out.push('\n');
    out
}

/// Where the licence section of a rendered `about.hbs` begins and ends.
pub const SECTION_START: &str = "<!-- licences -->";
pub const SECTION_END: &str = "<!-- /licences -->";

/// One document from cargo-about's per-binary renderings: the first document's frame around each
/// binary's licence section, under a heading naming the binary.
pub fn splice_licence_pages(pages: &[(&str, String)]) -> Result<String, String> {
    let (_, first) = pages.first().ok_or("no licence pages to join")?;
    let start = first
        .find(SECTION_START)
        .ok_or_else(|| format!("the licence template has no `{SECTION_START}` marker"))?;
    let end = first
        .rfind(SECTION_END)
        .ok_or_else(|| format!("the licence template has no `{SECTION_END}` marker"))?;
    let mut out = String::from(&first[..start]);
    for (binary, page) in pages {
        let s = page
            .find(SECTION_START)
            .ok_or_else(|| format!("{binary}'s licence page has no section marker"))?;
        let e = page
            .rfind(SECTION_END)
            .ok_or_else(|| format!("{binary}'s licence page has no section end"))?;
        out.push_str(&format!(
            "<section class=\"binary\">\n<h2>Linked into {binary}</h2>\n{}\n</section>\n",
            &page[s + SECTION_START.len()..e]
        ));
    }
    out.push_str(&first[end + SECTION_END.len()..]);
    Ok(out)
}

/// Everything a Pyreal package's `NOTICE.txt` states.
#[derive(Debug, Clone)]
pub struct PyrealFacts<'a> {
    pub version: &'a str,
    pub target: &'a str,
    pub commit: &'a str,
    /// The public repository, without a trailing slash.
    pub source_url: &'a str,
    pub crates: &'a [Crate],
    /// The project's MIT licence (the repository's top-level `LICENSE`).
    pub mit_licence: &'a str,
    /// The typefaces the package carries, as (typeface, what carries it and for what, licence
    /// text).
    pub fonts: &'a [(&'a str, &'a str, String)],
}

/// A Pyreal package's `NOTICE.txt`.
pub fn pyreal_notice(f: &PyrealFacts) -> String {
    let tag = super::version::Product::Pyreal.tag_for(f.version);
    let url = f.source_url.trim_end_matches('/');
    let title = format!("Pyreal {} ({})", f.version, f.target);
    let mut own = Vec::new();
    let mut third = Vec::new();
    for c in f.crates {
        let line = format!("  {} {} ({})", c.name, c.version, c.licence);
        if is_own(c) {
            own.push(line);
        } else {
            third.push(line);
        }
    }
    let mut out = format!(
        "{title}
{rule}

This package is Pyreal, an Asheron's Call client, made from the Dereth project's client. It is
free software under Dereth's MIT licence, which is in LICENSE beside this file and below.


SOURCE CODE

This build was made from commit {commit} of
  {url}
The source of exactly that commit:
  {url}/tree/{commit}
The release this package belongs to:
  {url}/releases/tag/{tag}


NO GAME DATA

This package contains no Asheron's Call files. The client reads the game's data files from an
install of your own.


PYREAL

{mit}",
        rule = "=".repeat(title.len()),
        commit = f.commit,
        mit = indent(f.mit_licence.trim_end()),
    );
    out.push_str(&font_sections(f.fonts));
    out.push_str(&built_from_section(&own, &third));
    out
}

/// Everything the plugin author's tool's `NOTICE.txt` states.
#[derive(Debug, Clone)]
pub struct ToolFacts<'a> {
    pub version: &'a str,
    pub target: &'a str,
    pub commit: &'a str,
    /// The public repository, without a trailing slash.
    pub source_url: &'a str,
    pub crates: &'a [Crate],
    /// The project's MIT license (the repository's top-level `LICENSE`).
    pub mit_license: &'a str,
}

/// The plugin author's tool's `NOTICE.txt`.
pub fn tool_notice(f: &ToolFacts) -> String {
    let tag = super::version::Product::Pyreal.tag_for(f.version);
    let url = f.source_url.trim_end_matches('/');
    let title = format!("pyreal-plugin {} ({})", f.version, f.target);
    let (own, third): (Vec<&Crate>, Vec<&Crate>) = f.crates.iter().partition(|c| is_own(c));
    let line = |c: &&Crate| format!("  {} {} ({})", c.name, c.version, c.licence);
    let mut out = format!(
        "{title}
{rule}

This package is pyreal-plugin, the plugin author's tool of Pyreal {version}: it makes a plugin
repository, checks a plugin as Pyreal checks it, packs it, releases it and lists it in a
plugin source. It is made from the Dereth project's plugin tool and is free software under
Dereth's MIT license, which is in LICENSE beside this file and below.


SOURCE CODE

This build was made from commit {commit} of
  {url}
The source of exactly that commit:
  {url}/tree/{commit}
The release this package belongs to:
  {url}/releases/tag/{tag}


PYREAL-PLUGIN

{mit}",
        rule = "=".repeat(title.len()),
        version = f.version,
        commit = f.commit,
        mit = indent(f.mit_license.trim_end()),
    );
    out.push_str(&built_from_section(
        &own.iter().map(line).collect::<Vec<_>>(),
        &third.iter().map(line).collect::<Vec<_>>(),
    ));
    out
}

/// A notice's sections for the typefaces a package carries, as (typeface, what carries it and for
/// what, license text).
fn font_sections(fonts: &[(&str, &str, String)]) -> String {
    let mut out = String::new();
    for (name, carried, text) in fonts {
        out.push_str(&format!(
            "

THE {upper} TYPEFACE

{carried}, under the SIL Open Font License, version 1.1:

{text}",
            upper = name.to_ascii_uppercase(),
            text = indent(text.trim_end()),
        ));
    }
    out
}

/// A notice's closing section: the crates the programs are built from, this project's and the
/// third parties', each a line.
fn built_from_section(own: &[String], third: &[String]) -> String {
    format!(
        "

WHAT THE BINARIES ARE BUILT FROM

This project's own crates:
{}

Third-party crates (their licence texts are in THIRD-PARTY-LICENSES.html):
{}
",
        own.join("\n"),
        third.join("\n")
    )
}
