//! Loot All: every item in the open chest or corpse moved to the player's pack, one at a time.
//!
//! Each move waits for the inventory lock to clear, which is the server's answer to the one
//! before. A refused item is skipped. A full pack or an overloaded player stops the run with a
//! chat line, and so does the container closing.

use std::collections::BTreeSet;

use pyreal_primitives::{ObjectId, ServerTime};

use super::requests::FEEDBACK_CHANNEL;
use super::SplitState;
use crate::world::World;
use crate::{Notice, NoticeSink, RequestSink};

/// Int property 5, `EncumbranceVal`.
const ENCUMBRANCE_VAL: u32 = 5;
/// Int property 230, `AugmentationIncreasedCarryingCapacity`.
const AUG_CARRYING_CAPACITY: u32 = 230;

/// A Loot All run over one ground container.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LootAll {
    pub container: ObjectId,
    /// The item whose move the server has not answered yet, and its stack size when sent.
    pub waiting_on: Option<(ObjectId, Option<u16>)>,
    /// Items the player was refused, left where they are.
    pub skipped: BTreeSet<ObjectId>,
}

/// Why a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LootAllEnd {
    Done,
    PackFull,
    Overloaded,
    Closed,
}

impl World {
    /// Start a Loot All run over `container`, the open ground container. Answers whether it started.
    pub fn start_loot_all(&mut self, container: ObjectId) -> bool {
        if container.0 == 0 || self.ground_object != Some(container) || self.player.is_none() {
            return false;
        }
        self.loot_all = Some(LootAll {
            container,
            ..LootAll::default()
        });
        true
    }

    /// Stop the run the player started.
    pub fn stop_loot_all(&mut self) {
        self.loot_all = None;
    }

    #[must_use]
    pub fn loot_all_running(&self) -> bool {
        self.loot_all.is_some()
    }

    /// One frame of the run: note the answer to the last move, then send the next one.
    pub fn loot_all_step(
        &mut self,
        req: &mut dyn RequestSink,
        out: &mut dyn NoticeSink,
        now: ServerTime,
    ) -> Option<LootAllEnd> {
        let run = self.loot_all.as_ref()?;
        let container = run.container;
        if self.ground_object != Some(container) {
            return Some(self.end_loot_all(out, LootAllEnd::Closed));
        }
        if !self.request_lock.is_idle() {
            return None;
        }
        if let Some((item, stack)) = run.waiting_on {
            // A stack that shrank was merged in part, and the rest is tried again.
            let refused = self.loot_items(container).contains(&item)
                && self.weenie(item).map(|w| w.pwd.stack_size) == Some(stack);
            let run = self.loot_all.as_mut()?;
            run.waiting_on = None;
            if refused {
                run.skipped.insert(item);
            }
        }
        let skipped = &self.loot_all.as_ref()?.skipped;
        let Some(item) = self
            .loot_items(container)
            .into_iter()
            .find(|i| !skipped.contains(i))
        else {
            return Some(self.end_loot_all(out, LootAllEnd::Done));
        };
        if self.loot_would_overload(item) {
            return Some(self.end_loot_all(out, LootAllEnd::Overloaded));
        }
        let player = self.player?;
        let stack = self.weenie(item).and_then(|w| w.pwd.stack_size);
        let whole = SplitState::whole_stack(stack.map_or(1, u32::from));
        if self.place_in_backpack(req, out, item, true, whole, now) {
            if let Some(run) = self.loot_all.as_mut() {
                run.waiting_on = Some((item, stack));
            }
            return None;
        }
        // A full pack stops the run; a pack the player has no room for is only skipped.
        let is_pack = self
            .weenie(item)
            .is_some_and(crate::weenie::Weenie::is_container);
        if !is_pack && self.spill_target(item, player, player, whole).is_none() {
            return Some(self.end_loot_all(out, LootAllEnd::PackFull));
        }
        if let Some(run) = self.loot_all.as_mut() {
            run.skipped.insert(item);
        }
        None
    }

    /// What Loot All takes from `container`: its loose items, then its packs.
    fn loot_items(&self, container: ObjectId) -> Vec<ObjectId> {
        self.inventory(container)
            .map(|inv| inv.items.iter().chain(&inv.containers).copied().collect())
            .unwrap_or_default()
    }

    /// Whether taking `item` puts the player past three times their capacity, the most a server takes.
    fn loot_would_overload(&self, item: ObjectId) -> bool {
        let Some(q) = self.player_qualities() else {
            return false;
        };
        let strength = pyreal_rules::attributes::inq_attribute(q, 1, false)
            .map_or(10, |s| i32::try_from(s).unwrap_or(i32::MAX));
        let capacity = pyreal_rules::burden::encumbrance_capacity_in(
            strength,
            q.inq_int(AUG_CARRYING_CAPACITY),
            &self.world_rules,
        );
        let burden = self
            .weenie(item)
            .and_then(|w| w.pwd.burden)
            .map_or(0, i32::from);
        q.inq_int(ENCUMBRANCE_VAL).saturating_add(burden) > capacity.saturating_mul(3)
    }

    fn end_loot_all(&mut self, out: &mut dyn NoticeSink, end: LootAllEnd) -> LootAllEnd {
        let left = self.loot_all.take().map_or(0, |r| r.skipped.len());
        let text = match end {
            LootAllEnd::PackFull => Some("Loot All stopped because your pack is full.".to_owned()),
            LootAllEnd::Overloaded => {
                Some("Loot All stopped because you are carrying too much.".to_owned())
            }
            LootAllEnd::Done if left == 1 => Some("Loot All left 1 item behind.".to_owned()),
            LootAllEnd::Done if left > 1 => Some(format!("Loot All left {left} items behind.")),
            _ => None,
        };
        if let Some(text) = text {
            out.emit(Notice::DisplayString {
                channel: FEEDBACK_CHANNEL,
                text,
                feedback: pyreal_client_contract::feedback::Feedback::LOCAL,
            });
        }
        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qualities::{Qualities, StatKey, StatType, StatValue};
    use crate::{RecordingRequests, RecordingSink, Request};
    use pyreal_protocol::types::{ContentProfile, PublicWeenieDesc};

    const PLAYER: ObjectId = ObjectId(1);
    const CORPSE: ObjectId = ObjectId(50);
    const NOW: ServerTime = ServerTime(1.0);

    /// A player with room for `slots` items over a corpse holding `items`, open on the ground.
    fn corpse_with(items: &[u32], slots: u8) -> World {
        let mut w = World::new();
        let mut add = |id: ObjectId, pwd: PublicWeenieDesc| {
            let mut wn = crate::weenie::Weenie::new(id);
            wn.pwd = pwd;
            wn.valid = true;
            w.tables.weenies.insert(id, wn);
        };
        add(
            PLAYER,
            PublicWeenieDesc {
                items_capacity: Some(slots),
                ..PublicWeenieDesc::default()
            },
        );
        add(
            CORPSE,
            PublicWeenieDesc {
                bitfield: pyreal_rules::weenie::bitfield::OPENABLE,
                ..PublicWeenieDesc::default()
            },
        );
        for id in items {
            add(
                ObjectId(*id),
                PublicWeenieDesc {
                    container_id: Some(CORPSE),
                    burden: Some(10),
                    ..PublicWeenieDesc::default()
                },
            );
        }
        w.player = Some(PLAYER);
        let contents: Vec<_> = items
            .iter()
            .map(|id| ContentProfile {
                iid: ObjectId(*id),
                container_properties: 0,
            })
            .collect();
        w.view_object_contents(CORPSE, &contents, &mut RecordingSink::default());
        w.ground_object = Some(CORPSE);
        w
    }

    fn sent_items(req: &RecordingRequests) -> Vec<ObjectId> {
        req.0
            .iter()
            .filter_map(|r| match r {
                Request::PutItemInContainer(m) => Some(m.item),
                _ => None,
            })
            .collect()
    }

    /// The server's yes: the lock lets go and the item leaves the corpse.
    fn taken(w: &mut World, item: ObjectId) {
        w.inventory_mut(CORPSE)
            .unwrap()
            .items
            .retain(|i| *i != item);
        w.request_lock.clear();
    }

    fn lines(out: &RecordingSink) -> Vec<String> {
        out.0
            .iter()
            .filter_map(|n| match n {
                Notice::DisplayString { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn loot_all_sends_one_move_and_waits_for_the_answer() {
        let mut w = corpse_with(&[60, 61], 102);
        let (mut req, mut out) = (RecordingRequests::default(), RecordingSink::default());
        assert!(w.start_loot_all(CORPSE));
        assert_eq!(w.loot_all_step(&mut req, &mut out, NOW), None);
        assert_eq!(sent_items(&req), [ObjectId(60)]);
        // Nothing more goes until the server answers.
        assert_eq!(w.loot_all_step(&mut req, &mut out, NOW), None);
        assert_eq!(sent_items(&req), [ObjectId(60)]);
        taken(&mut w, ObjectId(60));
        assert_eq!(w.loot_all_step(&mut req, &mut out, NOW), None);
        assert_eq!(sent_items(&req), [ObjectId(60), ObjectId(61)]);
        taken(&mut w, ObjectId(61));
        assert_eq!(
            w.loot_all_step(&mut req, &mut out, NOW),
            Some(LootAllEnd::Done)
        );
        assert!(!w.loot_all_running());
        assert!(lines(&out).is_empty(), "{:?}", lines(&out));
        let to_player = req.0.iter().all(|r| match r {
            Request::PutItemInContainer(m) => m.container == PLAYER,
            _ => true,
        });
        assert!(to_player, "every move is to the player's pack");
    }

    #[test]
    fn loot_all_skips_an_item_the_server_refuses() {
        let mut w = corpse_with(&[60, 61], 102);
        let (mut req, mut out) = (RecordingRequests::default(), RecordingSink::default());
        w.start_loot_all(CORPSE);
        w.loot_all_step(&mut req, &mut out, NOW);
        // Refused: the lock lets go and the item stays.
        w.request_lock.clear();
        w.loot_all_step(&mut req, &mut out, NOW);
        assert_eq!(sent_items(&req), [ObjectId(60), ObjectId(61)]);
        taken(&mut w, ObjectId(61));
        assert_eq!(
            w.loot_all_step(&mut req, &mut out, NOW),
            Some(LootAllEnd::Done)
        );
        assert_eq!(lines(&out), ["Loot All left 1 item behind."]);
    }

    #[test]
    fn loot_all_tries_again_after_a_stack_is_merged_in_part() {
        let mut w = corpse_with(&[60, 61], 102);
        w.weenie_mut(ObjectId(60)).unwrap().pwd.stack_size = Some(25);
        let (mut req, mut out) = (RecordingRequests::default(), RecordingSink::default());
        w.start_loot_all(CORPSE);
        w.loot_all_step(&mut req, &mut out, NOW);
        // The pack's stack took 5 of the 25, and 20 are left in the corpse.
        w.weenie_mut(ObjectId(60)).unwrap().pwd.stack_size = Some(20);
        w.request_lock.clear();
        w.loot_all_step(&mut req, &mut out, NOW);
        assert_eq!(sent_items(&req), [ObjectId(60), ObjectId(60)]);
    }

    #[test]
    fn loot_all_stops_when_the_pack_is_full() {
        let mut w = corpse_with(&[60, 61], 0);
        let (mut req, mut out) = (RecordingRequests::default(), RecordingSink::default());
        w.start_loot_all(CORPSE);
        assert_eq!(
            w.loot_all_step(&mut req, &mut out, NOW),
            Some(LootAllEnd::PackFull)
        );
        assert!(sent_items(&req).is_empty());
        assert_eq!(
            lines(&out).last().map(String::as_str),
            Some("Loot All stopped because your pack is full.")
        );
        assert!(!w.loot_all_running());
    }

    #[test]
    fn loot_all_stops_before_the_player_is_overloaded() {
        let mut w = corpse_with(&[60], 102);
        // Strength 10 carries 1,500, and a server takes up to three times that.
        let mut q = Qualities::new();
        q.set(
            StatKey::new(StatType::Int, ENCUMBRANCE_VAL),
            StatValue::Int(4_495),
        );
        w.weenie_mut(PLAYER).unwrap().qualities = Some(q);
        let (mut req, mut out) = (RecordingRequests::default(), RecordingSink::default());
        w.start_loot_all(CORPSE);
        assert_eq!(
            w.loot_all_step(&mut req, &mut out, NOW),
            Some(LootAllEnd::Overloaded)
        );
        assert!(sent_items(&req).is_empty());
        assert_eq!(
            lines(&out),
            ["Loot All stopped because you are carrying too much."]
        );
    }

    #[test]
    fn loot_all_stops_when_the_container_closes() {
        let mut w = corpse_with(&[60, 61], 102);
        let (mut req, mut out) = (RecordingRequests::default(), RecordingSink::default());
        w.start_loot_all(CORPSE);
        w.loot_all_step(&mut req, &mut out, NOW);
        taken(&mut w, ObjectId(60));
        w.ground_object = None;
        assert_eq!(
            w.loot_all_step(&mut req, &mut out, NOW),
            Some(LootAllEnd::Closed)
        );
        assert_eq!(sent_items(&req), [ObjectId(60)]);
        assert!(!w.loot_all_running());
    }

    #[test]
    fn loot_all_starts_only_on_the_open_ground_container() {
        let mut w = corpse_with(&[60], 102);
        assert!(!w.start_loot_all(ObjectId(51)));
        w.ground_object = None;
        assert!(!w.start_loot_all(CORPSE));
        assert!(!w.loot_all_running());
    }
}
