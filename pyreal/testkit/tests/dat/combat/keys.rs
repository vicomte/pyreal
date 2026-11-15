use pyreal_client_model::combat::CombatMode;
use pyreal_client_runtime::interaction::Interaction;
use pyreal_client_shell::input::InputShell;
use pyreal_input::spec::ControlChord;
use pyreal_input::{ActionId, InputMapId};
use pyreal_primitives::{DataId, LocalTime, ObjectId};

use super::maps;

const PLAYER: ObjectId = ObjectId(0x5410_0002);
const MONSTER: ObjectId = ObjectId(0x8410_0777);

/// The controls all three combat sections bind, discovered from the shipped keymap.
pub fn shared_controls(shell: &InputShell) -> Vec<ControlChord> {
    let section = |m: InputMapId| {
        shell
            .manager
            .keymap
            .section(m)
            .map(|s| s.bindings().to_vec())
            .unwrap_or_else(|| panic!("{m:?} has a shipped section"))
    };
    let melee = section(pyreal_input::combat::MELEE_COMBAT_MAP);
    let missile = section(pyreal_input::combat::MISSILE_COMBAT_MAP);
    let magic = section(pyreal_input::combat::MAGIC_COMBAT_MAP);
    let mut out: Vec<ControlChord> = melee
        .iter()
        .filter(|(qa, _)| {
            missile.iter().any(|(qb, _)| qa.is_conflicting(qb))
                && magic.iter().any(|(qb, _)| qa.is_conflicting(qb))
        })
        .map(|(q, _)| *q)
        .collect();
    out.sort_by_key(|q| q.control.offset());
    out.dedup_by_key(|q| q.control.offset());
    assert!(
        !out.is_empty(),
        "the three shipped sections share something"
    );
    out
}

/// **The calibration**, which every silence below rests on: the driver can produce an action
/// and the arm can move the world. The known positive is the key that toggles combat itself.
pub fn the_driver_and_the_arm_both_work() -> bool {
    use pyreal_client_contract::actions::mapped as ia;

    let mut shell = maps::shell();
    let mut d = maps::Driver::new();
    let qc = maps::the_shipped_control(
        &shell,
        pyreal_input::combat::COMBAT_MAP.0,
        ia::COMBAT_TOGGLE_COMBAT.0,
    );
    let (down, _) = d.press_release(&mut shell, &qc);
    if !down.iter().map(|e| e.id.0).eq([ia::COMBAT_TOGGLE_COMBAT.0]) {
        return false;
    }
    // ...and it reaches the world. This bench has no body, so the client queues the change
    // rather than taking it -- which names the branch the action reached, and is a stronger
    // reading than the mode simply moving.
    let mut bench = Bench::new(CombatMode::NonCombat);
    bench.drive(down, 100.0);
    bench.world().combat.pending_combat_mode == CombatMode::Melee
        && bench.world().combat.combat_mode == CombatMode::NonCombat
}

/// A player in `mode` with something attackable selected and the table the ready arm reads.
pub fn a_world(mode: CombatMode) -> pyreal_client_model::World {
    let mut w = pyreal_client_model::World::new();
    w.player = Some(PLAYER);
    w.tables
        .weenies
        .insert(PLAYER, pyreal_client_model::Weenie::new(PLAYER));
    w.tables
        .weenies
        .get_mut(PLAYER)
        .expect("just inserted")
        .pwd
        .name = "Aldis".into();
    w.tables.inventories.insert(
        PLAYER,
        pyreal_client_model::objects::ObjectInventory::new(PLAYER),
    );
    let mut m = pyreal_client_model::Weenie::new(MONSTER);
    m.pwd.name = "Mosswart".into();
    m.pwd.obj_type = pyreal_rules::weenie::item_type::CREATURE;
    m.pwd.bitfield |= pyreal_rules::weenie::bitfield::ATTACKABLE;
    w.tables.weenies.insert(MONSTER, m);
    w.set_selected_object(Some(MONSTER), false, &mut pyreal_client_model::NullSink);
    w.combat.combat_mode = mode;
    w.tables
        .weenies
        .get_mut(PLAYER)
        .expect("the player")
        .qualities
        .get_or_insert_with(pyreal_client_model::Qualities::new)
        .set(
            pyreal_client_model::qualities::StatKey::new(
                pyreal_client_model::qualities::StatType::Did,
                pyreal_client_model::combat::COMBAT_TABLE_DID,
            ),
            pyreal_client_model::qualities::StatValue::Did(DataId(0x3000_0000)),
        );
    if mode == CombatMode::Missile {
        w.combat.current_style = pyreal_client_model::combat::MISSILE_READY_STYLES[0];
    }
    assert!(
        w.player_in_ready_position(true, Some(false)),
        "the body must be able to attack at all, or every reading is a refusal it did not \
             intend"
    );
    w
}

/// The release event the input layer would build. The shipped combat bindings are one-shots
/// and produce none, so the arm is asked the question directly with the event it would have
/// been handed.
pub fn released_in(action: u32, _input_map: InputMapId) -> pyreal_client_runtime::actions::Action {
    pyreal_client_runtime::actions::Action {
        id: ActionId(action),
        phase: pyreal_client_runtime::actions::ActionPhase::End,
        extent: 1.0,
        repeats: 1,
    }
}

/// The height a swing carries, whichever kind of swing it is.
pub fn height_of(r: &pyreal_client_model::Request) -> Option<u32> {
    match r {
        pyreal_client_model::Request::TargetedMeleeAttack(m) => Some(m.attack_height),
        pyreal_client_model::Request::TargetedMissileAttack(m) => Some(m.attack_height),
        _ => None,
    }
}

/// The client's own interaction slot over a world, with no scene and no link: the requests are
/// counted, never sent.
pub struct Bench {
    inter: Interaction,
    objects: pyreal_client_runtime::objects::ObjectStream,
    store: std::sync::Arc<pyreal_dat::RetailDatStore>,
}

impl Bench {
    pub fn new(mode: CombatMode) -> Self {
        let mut objects = pyreal_client_runtime::objects::ObjectStream::default();
        objects.world = a_world(mode);
        Self {
            inter: Interaction::new(),
            objects,
            store: std::sync::Arc::new(pyreal_dat::testing::open_store_or_fail()),
        }
    }

    pub fn drive(&mut self, actions: Vec<pyreal_client_runtime::actions::Action>, now: f64) {
        let _ = pyreal_client_runtime::interaction::use_time(
            &mut self.inter,
            &self.store,
            None,
            &mut self.objects,
            None,
            actions,
            false,
            (800, 600),
            LocalTime(now),
        );
    }

    pub const fn world(&self) -> &pyreal_client_model::World {
        &self.objects.world
    }

    pub const fn height_changes(&self) -> u64 {
        self.inter.stats.attack_height_changes
    }

    pub const fn attacks_released(&self) -> u64 {
        self.inter.stats.attacks_released
    }

    pub const fn desired_power_changes(&self) -> u64 {
        self.inter.stats.desired_power_changes
    }

    /// What a frame with no link leaves behind: the requests that would have gone out.
    pub fn what_would_go_out(&self) -> Vec<pyreal_client_model::Request> {
        self.inter.last_sent.clone()
    }
}
