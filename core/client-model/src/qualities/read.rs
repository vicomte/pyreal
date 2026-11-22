//! The player's qualities as the shared inquiries read them (`pyreal_rules::quality::QualityRead`).
//!
//! Every read is the stored value: the attribute cache's slots, the skill table's records and the
//! int table, with the registry borrowed as it is kept.

use std::borrow::Cow;

use pyreal_protocol::types::qualities::{Attribute, SecondaryAttribute, Skill};
use pyreal_rules::enchant::EnchantmentRegistry;
use pyreal_rules::quality::QualityRead;

use super::Qualities;

impl QualityRead for Qualities {
    fn int(&self, id: u32) -> Option<i32> {
        self.ints.as_ref().and_then(|h| h.get(&id)).copied()
    }

    fn float(&self, id: u32) -> Option<f64> {
        self.floats.as_ref().and_then(|h| h.get(&id)).copied()
    }

    fn attribute(&self, id: u32) -> Option<Attribute> {
        Qualities::attribute(self, id)
    }

    fn attribute_2nd(&self, id: u32) -> Option<SecondaryAttribute> {
        Qualities::attribute_2nd(self, id)
    }

    fn skill(&self, id: u32) -> Option<Skill> {
        Qualities::skill(self, id).copied()
    }

    fn enchantments(&self) -> Cow<'_, EnchantmentRegistry> {
        Cow::Borrowed(&self.enchantments)
    }

    fn inq_int(&self, id: u32) -> i32 {
        Qualities::inq_int(self, id)
    }
}

#[cfg(test)]
mod tests {
    use crate::qualities::{Qualities, StatKey, StatType, StatValue};

    /// Oracle: §4's load enquiry. Strength defaults to 10 when the attribute cache is absent.
    #[test]
    fn inq_load_defaults_strength_to_ten() {
        let mut q = Qualities::new();
        q.set(StatKey::new(StatType::Int, 5), StatValue::Int(750));
        // capacity = 10 * 150 = 1500; 750 / 1500 = 0.5
        assert_eq!(pyreal_rules::burden::inq_load(&q), 0.5);
    }
}
