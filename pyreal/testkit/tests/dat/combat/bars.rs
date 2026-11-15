use pyreal_client_model::combat::{CombatMode, PowerBarMode, PowerBarNotice};
use pyreal_client_runtime::interaction::Interaction;
use pyreal_primitives::{LocalTime, ObjectId};

/// What the ramp reads at step `i`: one frame behind the clock, in tenths.
#[allow(clippy::cast_precision_loss)]
pub fn step_of(i: usize) -> f32 {
    i as f32 / 10.0
}

pub fn key(action: u32, start: bool) -> pyreal_client_runtime::actions::Action {
    pyreal_client_runtime::actions::Action {
        id: pyreal_input::ActionId(action),
        phase: if start {
            pyreal_client_runtime::actions::ActionPhase::Begin
        } else {
            pyreal_client_runtime::actions::ActionPhase::End
        },
        extent: 1.0,
        repeats: 1,
    }
}

/// The interaction slot over a world, with the charge notices each frame takes.
pub struct Bench {
    inter: Interaction,
    objects: pyreal_client_runtime::objects::ObjectStream,
    store: std::sync::Arc<pyreal_dat::RetailDatStore>,
    /// The notices the last frame took.
    taken: Vec<PowerBarNotice>,
}

impl Bench {
    pub fn new() -> Self {
        let mut objects = pyreal_client_runtime::objects::ObjectStream::default();
        objects.world = seed_world();
        Self {
            inter: Interaction::new(),
            objects,
            store: std::sync::Arc::new(pyreal_dat::testing::open_store_or_fail()),
            taken: Vec::new(),
        }
    }

    /// One frame: take what the previous frame produced, then run the combat system -- which
    /// is the order `App::frame` runs them in, and the reason every reading is one frame
    /// behind the clock.
    pub fn frame(&mut self, actions: Vec<pyreal_client_runtime::actions::Action>, t: f64) {
        self.taken = self.objects.world.combat.take_power_bar_notices();
        let _ = pyreal_client_runtime::interaction::use_time(
            &mut self.inter,
            &self.store,
            None,
            &mut self.objects,
            None,
            actions,
            false,
            (800, 600),
            LocalTime(t),
        );
    }

    pub fn combat(&self) -> &pyreal_client_model::combat::CombatState {
        &self.objects.world.combat
    }

    pub fn set_style(&mut self, style: u32) {
        self.objects.world.combat.current_style = style;
    }

    /// The level the last frame's notices set.
    pub fn drawn_level(&self) -> f32 {
        self.taken
            .iter()
            .rev()
            .find_map(|n| match n {
                PowerBarNotice::SetLevel { level, .. } => Some(*level),
                _ => None,
            })
            .expect("a notice set a level")
    }

    /// The mode every level notice the last frame took was sent in.
    pub fn modes(&self) -> Vec<PowerBarMode> {
        self.taken
            .iter()
            .filter_map(|n| match n {
                PowerBarNotice::SetLevel { mode, .. } => Some(*mode),
                _ => None,
            })
            .collect()
    }

    /// Turn the advanced interface on through the client's own producer: the option, and then
    /// a mode change, which is the only writer of the cached flag in a running client.
    pub fn enter_advanced_combat(&mut self) {
        let w = &mut self.objects.world;
        w.player_system.options.set(
            pyreal_client_model::player::options::option::ADVANCED_COMBAT_UI,
            true,
        );
        assert!(
            !w.combat.advanced_combat_mode,
            "the option alone changes nothing"
        );
        let mut req = pyreal_client_model::NullRequests;
        let mut sink = pyreal_client_model::NullSink;
        w.set_combat_mode(
            &mut req,
            &mut sink,
            CombatMode::NonCombat,
            false,
            true,
            false,
        )
        .expect("peace is always compatible");
        w.set_combat_mode(&mut req, &mut sink, CombatMode::Melee, false, true, false)
            .expect("unarmed melee is always compatible");
        assert!(
            w.combat.advanced_combat_mode,
            "the mode change re-read the option; without that every reading below would be \
                 about a field written by hand"
        );
        assert_ne!(w.combat.power_bar_mode, PowerBarMode::Combat);
    }
}

/// A player in melee with an attackable creature selected, carrying the table the melee ready
/// arm reads -- which is the state a swing is actually allowed from.
fn seed_world() -> pyreal_client_model::World {
    let mut w = pyreal_client_model::World::new();
    let player = ObjectId(0x5430_0002);
    let monster = ObjectId(0x8430_0777);
    w.player = Some(player);
    w.tables
        .weenies
        .insert(player, pyreal_client_model::Weenie::new(player));
    w.tables
        .weenies
        .get_mut(player)
        .expect("just inserted")
        .pwd
        .name = "Aldis".into();
    w.tables.inventories.insert(
        player,
        pyreal_client_model::objects::ObjectInventory::new(player),
    );
    let mut m = pyreal_client_model::Weenie::new(monster);
    m.pwd.name = "Mosswart".into();
    m.pwd.obj_type = pyreal_rules::weenie::item_type::CREATURE;
    m.pwd.bitfield |= pyreal_rules::weenie::bitfield::ATTACKABLE;
    w.tables.weenies.insert(monster, m);
    w.set_selected_object(Some(monster), false, &mut pyreal_client_model::NullSink);
    w.combat.combat_mode = CombatMode::Melee;
    w.tables
        .weenies
        .get_mut(player)
        .expect("the player")
        .qualities
        .get_or_insert_with(pyreal_client_model::Qualities::new)
        .set(
            pyreal_client_model::qualities::StatKey::new(
                pyreal_client_model::qualities::StatType::Did,
                pyreal_client_model::combat::COMBAT_TABLE_DID,
            ),
            pyreal_client_model::qualities::StatValue::Did(pyreal_primitives::DataId(0x3000_0000)),
        );
    assert!(
        w.player_in_ready_position(true, Some(false)),
        "the body must be able to attack at all, or every reading is a refusal it did not \
             intend"
    );
    w
}
