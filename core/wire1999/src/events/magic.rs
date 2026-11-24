//! Spells and enchantments. The bodies are the later ones; enchantment ids gain layer 1.

use pyreal_protocol::qualities::{
    MagicDispelEnchantment, MagicDispelMultipleEnchantments, MagicRemoveEnchantment,
    MagicRemoveMultipleEnchantments, MagicRemoveSpell, MagicUpdateEnchantment,
    MagicUpdateMultipleEnchantments, MagicUpdateSpell,
};
use pyreal_protocol::types::qualities::Enchantment;
use pyreal_protocol::{MessageError, Reader};

use super::{codec, Handled, Later};
use crate::player::{layer, layered};

/// `0x004C` spell learned, also how the client learns a cast happened.
pub const SPELL_LEARNED: u32 = 0x004C;
/// `0x004D` spell removed.
pub const SPELL_REMOVED: u32 = 0x004D;
/// `0x004E` one enchantment added or updated.
pub const ENCHANTMENT: u32 = 0x004E;
/// `0x01A5` several added.
pub const ENCHANTMENTS: u32 = 0x01A5;
/// `0x004F` one removed.
pub const REMOVED: u32 = 0x004F;
/// `0x01A6` several removed.
pub const REMOVED_SEVERAL: u32 = 0x01A6;
/// `0x01A4` one dispelled.
pub const DISPELLED: u32 = 0x01A4;
/// `0x01AE` several dispelled.
pub const DISPELLED_SEVERAL: u32 = 0x01AE;

pub(super) fn translate(opcode: u32, message: &[u8]) -> Option<Result<Handled, MessageError>> {
    let one = |r: &mut Reader<'_>| r.u32();
    let list = |r: &mut Reader<'_>| -> Result<Vec<u32>, MessageError> {
        Ok(r.packed_list(Reader::u32)?
            .into_iter()
            .map(layered)
            .collect())
    };
    let enchantment = |r: &mut Reader<'_>| -> Result<Enchantment, MessageError> {
        let mut e = Enchantment::read(r)?;
        layer(&mut e);
        Ok(e)
    };
    let out = match opcode {
        SPELL_LEARNED => read(message, one).and_then(|id| {
            Later::of(&MagicUpdateSpell {
                layered_spell_id: id,
            })
        }),
        SPELL_REMOVED => read(message, one).and_then(|id| {
            Later::of(&MagicRemoveSpell {
                layered_spell_id: id,
            })
        }),
        ENCHANTMENT => {
            read(message, enchantment).and_then(|e| Later::of(&MagicUpdateEnchantment(e)))
        }
        ENCHANTMENTS => read(message, |r| r.packed_list(enchantment))
            .and_then(|l| Later::of(&MagicUpdateMultipleEnchantments(l))),
        REMOVED => read(message, one).and_then(|id| {
            Later::of(&MagicRemoveEnchantment {
                layered_spell_id: layered(id),
            })
        }),
        REMOVED_SEVERAL => {
            read(message, list).and_then(|l| Later::of(&MagicRemoveMultipleEnchantments(l)))
        }
        DISPELLED => read(message, one).and_then(|id| {
            Later::of(&MagicDispelEnchantment {
                layered_spell_id: layered(id),
            })
        }),
        DISPELLED_SEVERAL => {
            read(message, list).and_then(|l| Later::of(&MagicDispelMultipleEnchantments(l)))
        }
        _ => return None,
    };
    Some(out.map(Handled::from))
}

/// Reads a whole 1999 body with `f`.
pub(super) fn read<T>(
    message: &[u8],
    f: impl FnOnce(&mut Reader<'_>) -> Result<T, MessageError>,
) -> Result<T, MessageError> {
    let mut r = codec::reader(message)?;
    let v = f(&mut r)?;
    codec::finish(&mut r)?;
    Ok(v)
}
