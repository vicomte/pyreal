//! Permission to edit a live object's inscription.

/// Displayed hook contents cannot grant permission to write on the live object.
pub fn inscription_editable(
    live_facts: Option<(bool, u32)>,
    owned_by_player: bool,
    viewer_is_psr: bool,
    scribe: &str,
    player: &str,
) -> bool {
    live_facts.is_some_and(|(inscribable, _)| inscribable)
        && (viewer_is_psr
            || (owned_by_player && (scribe.is_empty() || scribe.eq_ignore_ascii_case(player))))
}
