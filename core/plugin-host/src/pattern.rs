//! Regular expressions for plugins (`text.match`), in the dialect VTank metas and loot profiles
//! are written in.
//!
//! The dialect is the common one: classes, groups, named groups (`(?<name>…)`), alternation and
//! repetition. Two differences from this host's matcher are bridged: a backslash before `<` or
//! `>` is a plain angle bracket there (here it would be a word boundary), and a match may be asked
//! for without regard to case.

use std::collections::BTreeMap;

use pyreal_plugin_api::Value;
use regex_automata::meta::{self, Regex};
use regex_automata::util::syntax;
use regex_automata::PatternID;

/// The most patterns one plugin keeps compiled; the oldest is dropped past it.
pub const CACHE: usize = 256;

/// The most compiled size one pattern may take: a list of a few hundred names fits.
pub const PATTERN_SIZE: usize = 1 << 20;

/// The most compiled size a plugin's kept patterns, or one loot profile's patterns, take together.
pub const HELD: usize = 16 << 20;

/// `pattern` compiled, its automata at most `limit` bytes each.
pub(crate) fn build(
    pattern: &str,
    ignore_case: bool,
    limit: usize,
) -> Result<Regex, Box<meta::BuildError>> {
    Regex::builder()
        .syntax(syntax::Config::new().case_insensitive(ignore_case))
        .configure(
            Regex::config()
                .nfa_size_limit(Some(limit))
                .hybrid_cache_capacity(PATTERN_SIZE),
        )
        .build(pattern)
        .map_err(Box::new)
}

/// `pattern` in this host's syntax.
#[must_use]
pub fn translate(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(n @ ('<' | '>')) => out.push(n),
                Some(n) => {
                    out.push('\\');
                    out.push(n);
                }
                None => out.push_str("\\\\"),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// A plugin's compiled patterns.
#[derive(Debug, Default)]
pub struct Patterns {
    compiled: BTreeMap<(String, bool), Regex>,
    order: std::collections::VecDeque<(String, bool)>,
    /// The compiled size of those kept, at most [`HELD`].
    held: usize,
}

impl Patterns {
    /// `pattern` compiled (from the cache when it has been before).
    ///
    /// # Errors
    /// The pattern does not compile.
    pub fn get(&mut self, pattern: &str, ignore_case: bool) -> Result<&Regex, String> {
        let key = (pattern.to_owned(), ignore_case);
        if !self.compiled.contains_key(&key) {
            let re = build(&translate(pattern), ignore_case, PATTERN_SIZE)
                .map_err(|e| format!("the pattern {pattern:?} does not compile: {e}"))?;
            let size = re.memory_usage();
            while self.order.len() >= CACHE || self.held + size > HELD {
                let Some(old) = self.order.pop_front() else {
                    break;
                };
                if let Some(gone) = self.compiled.remove(&old) {
                    self.held -= gone.memory_usage();
                }
            }
            self.held += size;
            self.order.push_back(key.clone());
            self.compiled.insert(key.clone(), re);
        }
        Ok(&self.compiled[&key])
    }
}

/// The first match of `re` in `text`: `{ text, groups = { … }, named = { … } }`, or nil. A group
/// that took part in no match is an empty string, as the metas expect.
#[must_use]
pub fn first_match(re: &Regex, text: &str) -> Value {
    let mut caps = re.create_captures();
    re.captures(text, &mut caps);
    let Some(whole) = caps.get_match() else {
        return Value::Nil;
    };
    let whole = &text[whole.range()];
    let group = |span: Option<regex_automata::Span>| span.map_or("", |s| &text[s.range()]);
    let groups = (1..caps.group_len())
        .map(|i| Value::from(group(caps.get_group(i)).to_owned()))
        .collect();
    let mut named = Value::map();
    for name in re.group_info().pattern_names(PatternID::ZERO).flatten() {
        named = named.with(name, group(caps.get_group_by_name(name)).to_owned());
    }
    Value::map()
        .with("text", whole.to_owned())
        .with("groups", Value::List(groups))
        .with("named", named)
}

#[cfg(test)]
mod tests {
    use super::{first_match, Patterns};

    #[test]
    fn the_metas_dialect_matches_with_named_groups_and_escaped_brackets() {
        let mut p = Patterns::default();
        let re = p
            .get(r#"^\[Fellowship\] .* (say|says)\, \"(?<nav>.*)\"$"#, false)
            .unwrap()
            .clone();
        let m = first_match(&re, "[Fellowship] Bob says, \"go north\"");
        assert_eq!(m.get("named").get("nav").as_str(), Some("go north"));
        assert_eq!(m.get("groups").as_list().unwrap()[0].as_str(), Some("says"));
        let re = p.get(r"\<Tell:(?<n>[^\<]*)\>", false).unwrap().clone();
        assert_eq!(
            first_match(&re, "<Tell:Bob>")
                .get("named")
                .get("n")
                .as_str(),
            Some("Bob")
        );
        let re = p.get("hello", true).unwrap().clone();
        assert!(!first_match(&re, "HELLO there").is_nil());
        assert!(p.get("(", false).is_err());
    }

    /// The cache keeps no more compiled size than [`super::HELD`], dropping the oldest past it.
    #[test]
    fn the_cache_holds_a_bounded_compiled_size() {
        let mut p = Patterns::default();
        for i in 0..100 {
            p.get(&format!(r"\w{{12}}{i}"), false).expect("compiles");
        }
        assert!(p.compiled.len() < 40, "{} held", p.compiled.len());
        let re = p.get(r"\w{12}99", false).unwrap().clone();
        assert!(
            !first_match(&re, "abcdefghijkl99").is_nil(),
            "the newest is kept"
        );
    }
}
