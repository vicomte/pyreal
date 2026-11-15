//! `@fog` (also `/fog`), this client's own command for the haze over the world. Retail has none.
//!
//! It sets the Fog Density option (`Fidelity.FogDensity`) as the graphics page does: 1 the world
//! fog's own haze, 0 none. Nothing is sent. This module holds the command's words.

/// What a typed `@fog` asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FogCommand {
    /// Say how thick the haze is.
    Show,
    /// Draw the haze this thick from now on, 1 the world fog's own; 0 none.
    Set(f32),
    /// Back to the option's default.
    Default,
}

/// The thickest haze `@fog` takes.
pub const MOST: f32 = 3.0;

/// What `@fog` takes: the tail of every refusal.
pub const USAGE: &str =
    "@fog takes a number from 0 to 3 (1 is the world's own fog), off or default.";

/// Read a typed `@fog`'s arguments. `Err` is the one line that says what was wrong.
///
/// # Errors
///
/// More than one word, or a word that is neither a number in range nor `off` or `default`.
pub fn parse(args: &[String]) -> Result<FogCommand, String> {
    let word = match args {
        [] => return Ok(FogCommand::Show),
        [word] => word.trim(),
        _ => return Err(format!("Too many words after @fog. {USAGE}")),
    };
    if word.eq_ignore_ascii_case("off") || word.eq_ignore_ascii_case("none") {
        return Ok(FogCommand::Set(0.0));
    }
    if word.eq_ignore_ascii_case("default") || word.eq_ignore_ascii_case("reset") {
        return Ok(FogCommand::Default);
    }
    match word.parse::<f32>() {
        Ok(v) if v.is_finite() && (0.0..=MOST).contains(&v) => Ok(FogCommand::Set(v)),
        _ => Err(format!("\"{word}\" is not a fog density. {USAGE}")),
    }
}

/// The answer once the haze is `density` thick (1 the world fog's own), named `level`.
#[must_use]
pub fn describe(density: f32, level: &str) -> String {
    if density <= 0.0 {
        "Fog density: off".to_owned()
    } else {
        format!("Fog density: {density:.2} ({level})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        crate::cmd::interp::find_all_words(line)
    }

    #[test]
    fn a_number_off_or_default_sets_the_haze_and_anything_else_is_refused() {
        assert_eq!(parse(&[]), Ok(FogCommand::Show));
        assert_eq!(parse(&words("0.5")), Ok(FogCommand::Set(0.5)));
        assert_eq!(parse(&words("OFF")), Ok(FogCommand::Set(0.0)));
        assert_eq!(parse(&words("default")), Ok(FogCommand::Default));
        for line in ["9", "-1", "thick", "0.5 1"] {
            let refusal = parse(&words(line)).expect_err(line);
            assert!(refusal.ends_with(USAGE), "{line}: {refusal}");
        }
        assert_eq!(describe(0.0, "Off"), "Fog density: off");
        assert_eq!(describe(0.4, "Clear"), "Fog density: 0.40 (Clear)");
    }
}
