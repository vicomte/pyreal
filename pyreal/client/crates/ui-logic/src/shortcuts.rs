//! The shortcut bar's actions: which slot an input action uses, or the shortcut it makes.

/// What an input action asked the toolbar to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutAction {
    /// Use the shortcut in `slot`, primary or secondary.
    Use { slot: u32, primary: bool },
    /// Create a shortcut to the selected object in the first free slot (`−1`) — "make a shortcut
    /// to the selected item".
    CreateToSelected,
}

/// The actions dispatched for global message 1 are listed below.
///
/// | Action range | Effect |
/// |---|---|
/// | `0x10000042` … `0x1000004D` | use slot `action − 0x10000042`, primary — slots 0–11 |
/// | `0x1000004E` … `0x10000059` | use slot `action − 0x1000004E`, secondary — slots 0–11 |
/// | `0x10000132` … `0x10000137` | use slots 12…17, primary |
/// | `0x10000138` … `0x1000013D` | use slots 12…17, secondary |
/// | `0x1000010D` | create a shortcut to the selected object |
///
/// Note the overlap the two tables describe: the first range covers **twelve** slots (0–11) even
/// though bank 1 holds nine, so actions `0x1000004B`…`0x1000004D` reach slots 9–11, which are the
/// first three of bank 2. That is what the client does and it is reproduced, not tidied.
#[must_use]
pub fn shortcut_action(action: u32) -> Option<ShortcutAction> {
    match action {
        0x1000_0042..=0x1000_004D => Some(ShortcutAction::Use {
            slot: action - 0x1000_0042,
            primary: true,
        }),
        0x1000_004E..=0x1000_0059 => Some(ShortcutAction::Use {
            slot: action - 0x1000_004E,
            primary: false,
        }),
        0x1000_0132..=0x1000_0137 => Some(ShortcutAction::Use {
            slot: 12 + (action - 0x1000_0132),
            primary: true,
        }),
        0x1000_0138..=0x1000_013D => Some(ShortcutAction::Use {
            slot: 12 + (action - 0x1000_0138),
            primary: false,
        }),
        0x1000_010D => Some(ShortcutAction::CreateToSelected),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercise shortcut actions at every range boundary. The four ranges are separate
    /// and must not be collapsed.
    #[test]
    fn the_four_action_ranges_map_to_the_documented_slots_and_never_overlap() {
        use ShortcutAction::{CreateToSelected, Use};
        assert_eq!(
            shortcut_action(0x1000_0042),
            Some(Use {
                slot: 0,
                primary: true
            })
        );
        assert_eq!(
            shortcut_action(0x1000_004D),
            Some(Use {
                slot: 11,
                primary: true
            })
        );
        assert_eq!(
            shortcut_action(0x1000_004E),
            Some(Use {
                slot: 0,
                primary: false
            })
        );
        assert_eq!(
            shortcut_action(0x1000_0059),
            Some(Use {
                slot: 11,
                primary: false
            })
        );
        assert_eq!(
            shortcut_action(0x1000_0132),
            Some(Use {
                slot: 12,
                primary: true
            })
        );
        assert_eq!(
            shortcut_action(0x1000_0137),
            Some(Use {
                slot: 17,
                primary: true
            })
        );
        assert_eq!(
            shortcut_action(0x1000_0138),
            Some(Use {
                slot: 12,
                primary: false
            })
        );
        assert_eq!(
            shortcut_action(0x1000_013D),
            Some(Use {
                slot: 17,
                primary: false
            })
        );
        assert_eq!(shortcut_action(0x1000_010D), Some(CreateToSelected));
        // Just outside every range.
        for a in [
            0x1000_0041,
            0x1000_005A,
            0x1000_0131,
            0x1000_013E,
            0x1000_010C,
        ] {
            assert_eq!(shortcut_action(a), None, "{a:#X}");
        }
        // Primary and secondary are genuinely different ranges reaching the same slot.
        assert_ne!(shortcut_action(0x1000_0042), shortcut_action(0x1000_004E));
        // Every slot 0..=17 is reachable.
        let mut seen: Vec<u32> = Vec::new();
        for a in 0x1000_0000_u32..0x1000_0200 {
            if let Some(Use { slot, .. }) = shortcut_action(a) {
                seen.push(slot);
            }
        }
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen, (0..18).collect::<Vec<u32>>());
    }
}
