//! The radar's palette, its blip colours and shapes, and its range.

use pyreal_client_contract::radar as shared;
use pyreal_client_contract::view::RadarEntry;

/// One RGBA colour constant built into the client. Alpha is 1.0 for every entry in the radar
/// palette.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadarColor {
    pub rgb: (f32, f32, f32),
    /// The 8-bit form, `0xRRGGBB`.
    pub hex: u32,
}

const fn rc(rgb: (f32, f32, f32), hex: u32) -> RadarColor {
    RadarColor { rgb, hex }
}

/// The rgba color radar blue.
pub const BLUE: RadarColor = rc((0.25, 0.66, 1.00), 0x40A8FF);
/// The rgba color radar gold.
pub const GOLD: RadarColor = rc((1.00, 0.67, 0.00), 0xFFAB00);
/// The rgba color radar yellow.
pub const YELLOW: RadarColor = rc((1.00, 1.00, 0.50), 0xFFFF80);
/// The rgba color radar white.
pub const WHITE: RadarColor = rc((1.00, 1.00, 1.00), 0xFFFFFF);
/// The rgba color radar red.
pub const RED: RadarColor = rc((1.00, 0.25, 0.39), 0xFF4063);
/// The rgba color radar purple.
pub const PURPLE: RadarColor = rc((0.75, 0.39, 1.00), 0xBF63FF);
/// The rgba color radar pink.
pub const PINK: RadarColor = rc((1.00, 0.66, 0.75), 0xFFA8BF);
/// The rgba color radar green.
pub const GREEN: RadarColor = rc((0.00, 0.50, 0.25), 0x008040);
/// The rgba color radar cyan.
pub const CYAN: RadarColor = rc((0.00, 1.00, 1.00), 0x00FFFF);
/// The rgba color radar bright green.
pub const BRIGHT_GREEN: RadarColor = rc((0.00, 1.00, 0.00), 0x00FF00);

/// The ten base colours.
pub const BASE_PALETTE: [RadarColor; 10] = [
    BLUE,
    GOLD,
    YELLOW,
    WHITE,
    RED,
    PURPLE,
    PINK,
    GREEN,
    CYAN,
    BRIGHT_GREEN,
];

/// The thirteen semantic aliases, aliased at static-init time by the client.
///
/// Several aliases collide deliberately (Admin = Sentinel = Cyan, NPC = Vendor = Yellow,
/// Fellowship = FellowshipLeader = BrightGreen). Do not "fix" them.
pub mod semantic {
    use super::{RadarColor, BLUE, BRIGHT_GREEN, CYAN, GOLD, PINK, PURPLE, RED, WHITE, YELLOW};
    /// = White.
    pub const DEFAULT: RadarColor = WHITE;
    /// = Cyan.
    pub const ADMIN: RadarColor = CYAN;
    /// = Pink.
    pub const ADVOCATE: RadarColor = PINK;
    /// = Gold.
    pub const CREATURE: RadarColor = GOLD;
    /// = Blue.
    pub const LIFE_STONE: RadarColor = BLUE;
    /// = Yellow.
    pub const NPC: RadarColor = YELLOW;
    /// = Red.
    pub const PLAYER_KILLER: RadarColor = RED;
    /// = Purple.
    pub const PORTAL: RadarColor = PURPLE;
    /// = Cyan, the same colour as [`ADMIN`].
    pub const SENTINEL: RadarColor = CYAN;
    /// = Yellow, the same colour as [`NPC`].
    pub const VENDOR: RadarColor = YELLOW;
    /// = BrightGreen.
    pub const FELLOWSHIP: RadarColor = BRIGHT_GREEN;
    /// = BrightGreen, the same colour as [`FELLOWSHIP`].
    pub const FELLOWSHIP_LEADER: RadarColor = BRIGHT_GREEN;
    /// = Pink, the same colour as [`ADVOCATE`].
    pub const PK_LITE: RadarColor = PINK;
}

/// The object bitfield bits the radar reads.
///
/// Defined in [`pyreal_client_contract::radar::bitfield`], because `pyreal_client_runtime::hud` reads `PK`,
/// `PK_LITE` and `PLAYER` from here and both sides must name the same mask.
pub use pyreal_client_contract::radar::bitfield;

// -------------------------------------------------------------------------------------------
// The filter
// -------------------------------------------------------------------------------------------

/// `RadarEnum`, the value an object's radar field carries.
///
/// Defined in [`pyreal_client_contract::radar::radar_enum`], beside [`bitfield`], for the same
/// reason: `pyreal_client_runtime::hud` reads `UNDEF`.
pub use pyreal_client_contract::radar::radar_enum;

/// **The radar's filter**, and the reason a
/// retail radar plots a handful of blips in a scene holding hundreds of objects.
///
/// The client's test is two conditions and nothing else: the object has a physics object, and
/// its radar field is `ShowMovement`, `ShowAlways` or `ShowAttacking`.
///
/// The client calls it before it will build a [`RadarEntry`]'s radar-info record
/// at all, so a rejected object never enters the radar's info list and can never be drawn, hovered or
/// tooltipped.
///
/// **Measured.** In the recorded training-dungeon session at its most populated
/// instant, 285 objects carry a `PublicWeenieDesc`; 179 of them were sent with no
/// `RADAR_ENUM` field and 5 more with `Undef`, so **184 of 285 are filtered here** before the range
/// cull runs at all. The 101 survivors are the NPCs, vendors, creatures, portals, life stones and
/// bind stones — exactly the categories the retail radar shows.
///
/// "Has a physics object" maps onto [`RadarEntry::in_world`], which is the same "the server has
/// placed this object" fact our object stream carries; the draw loop re-tests the cell for the same
/// reason.
#[must_use]
pub fn inq_showable_on_radar(o: &RadarEntry) -> bool {
    shared::showable(o)
}

// -------------------------------------------------------------------------------------------
// The blip colour
// -------------------------------------------------------------------------------------------

/// The radar panel's blip colour, in the client's exact decision order.
#[must_use]
pub fn get_blip_color(o: Option<&RadarEntry>) -> RadarColor {
    match shared::color_role(o).index() {
        1 => BLUE,
        2 => GOLD,
        3 => WHITE,
        4 => PURPLE,
        5 => RED,
        6 => PINK,
        7 => GREEN,
        8 => YELLOW,
        9 => CYAN,
        10 => BRIGHT_GREEN,
        _ => semantic::DEFAULT,
    }
}

// -------------------------------------------------------------------------------------------
// Blip shape
// -------------------------------------------------------------------------------------------

/// The radar blip shape — two names per value, geometric and semantic; the *semantic* names are
/// what the code uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum BlipShape {
    /// 0 `Undef` / `Undef` — drawn as nothing.
    Undef = 0,
    /// 1 `Circle` / `Circle` — the draw point.
    Circle = 1,
    /// 2 `Box` / `AllegianceMember` — the draw hollow.
    AllegianceMember = 2,
    /// 3 `X` / `Threat` — the draw x.
    Threat = 3,
    /// 4 `Plus` / `Default` — the draw cross.
    Default = 4,
    /// 5 `Triangle` / `FellowshipLeader` — the draw triangle.
    FellowshipLeader = 5,
    /// 6 `InvertedTriangle` / `Fellowship` — the draw inverted triangle.
    Fellowship = 6,
    /// 7 `XBox` / `ThreatAllegiance` — the draw x box.
    ThreatAllegiance = 7,
}

impl BlipShape {
    /// The *geometric* name, for cross-referencing the enum page.
    #[must_use]
    pub const fn geometric(self) -> &'static str {
        match self {
            Self::Undef => "Undef",
            Self::Circle => "Circle",
            Self::AllegianceMember => "Box",
            Self::Threat => "X",
            Self::Default => "Plus",
            Self::FellowshipLeader => "Triangle",
            Self::Fellowship => "InvertedTriangle",
            Self::ThreatAllegiance => "XBox",
        }
    }

    /// The pixels one blip covers, relative to its centre.
    ///
    /// Every one is a 1×1 area fill; the list is in the order the drawing function
    /// issues them, which is the order a pixel-exact comparison needs.
    #[must_use]
    pub fn pixels(self) -> Vec<(i32, i32)> {
        const EDGES: [(i32, i32); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
        const CORNERS: [(i32, i32); 4] = [(1, 1), (-1, -1), (-1, 1), (1, -1)];
        match self {
            Self::Undef => Vec::new(),
            Self::Circle => vec![(0, 0)],
            Self::Default => std::iter::once((0, 0)).chain(EDGES).collect(),
            Self::Threat => std::iter::once((0, 0)).chain(CORNERS).collect(),
            Self::AllegianceMember => EDGES.into_iter().chain(CORNERS).collect(),
            Self::ThreatAllegiance => EDGES
                .into_iter()
                .chain(CORNERS)
                .chain(std::iter::once((0, 0)))
                .collect(),
            Self::FellowshipLeader => vec![(0, 0), (-1, 1), (0, 1), (1, 1)],
            Self::Fellowship => vec![(0, 0), (-1, -1), (0, -1), (1, -1)],
        }
    }
}

/// The radar panel's blip shape, in the client's exact decision order.
///
/// `player` is the player's own entry, or `None` before `PlayerDescReceived`.
#[must_use]
pub fn get_blip_shape(o: Option<&RadarEntry>, player: Option<&RadarEntry>) -> BlipShape {
    let viewer = player.map(|p| shared::Viewer {
        pk: p.is_pk,
        pk_lite: p.is_pk_lite,
    });
    match shared::shape_role(o, viewer) {
        shared::ShapeRole::Hidden => BlipShape::Undef,
        shared::ShapeRole::Ordinary => BlipShape::Default,
        shared::ShapeRole::Allegiance => BlipShape::AllegianceMember,
        shared::ShapeRole::Threat => BlipShape::Threat,
        shared::ShapeRole::FellowshipLeader => BlipShape::FellowshipLeader,
        shared::ShapeRole::Fellowship => BlipShape::Fellowship,
    }
}

/// The radar radius: 75 units outdoors, 25 indoors. The pixel radius does
/// not change, only the scale.
///
/// Defined in [`pyreal_client_contract::radar`]: it is the radius the *selection* and
/// *speech* sweeps in `pyreal_client_runtime::{hud, interaction}` use, at seven call sites, and it is one
/// branch over a `bool`.
pub use pyreal_client_contract::radar::radar_range;

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(bitfield: u32) -> RadarEntry {
        RadarEntry {
            bitfield,
            in_world: true,
            ..Default::default()
        }
    }

    /// Oracle: the ten base-colour RGB values and thirteen semantic aliases, including the three
    /// deliberate collisions.
    #[test]
    fn the_palette_is_ten_base_colours_and_thirteen_aliases_with_the_documented_collisions() {
        assert_eq!(BASE_PALETTE.len(), 10);
        let hexes: Vec<u32> = BASE_PALETTE.iter().map(|c| c.hex).collect();
        assert_eq!(
            hexes,
            vec![
                0x40A8FF, 0xFFAB00, 0xFFFF80, 0xFFFFFF, 0xFF4063, 0xBF63FF, 0xFFA8BF, 0x008040,
                0x00FFFF, 0x00FF00
            ]
        );
        assert_eq!(semantic::DEFAULT.hex, WHITE.hex);
        assert_eq!(semantic::LIFE_STONE.hex, BLUE.hex);
        assert_eq!(semantic::PORTAL.hex, PURPLE.hex);
        assert_eq!(semantic::PLAYER_KILLER.hex, RED.hex);
        assert_eq!(semantic::CREATURE.hex, GOLD.hex);
        // The three deliberate collisions.
        assert_eq!(
            semantic::ADMIN.hex,
            semantic::SENTINEL.hex,
            "Admin = Sentinel = Cyan"
        );
        assert_eq!(
            semantic::NPC.hex,
            semantic::VENDOR.hex,
            "NPC = Vendor = Yellow"
        );
        assert_eq!(
            semantic::FELLOWSHIP.hex,
            semantic::FELLOWSHIP_LEADER.hex,
            "Fellowship = FellowshipLeader = BrightGreen"
        );
        assert_eq!(
            semantic::ADVOCATE.hex,
            semantic::PK_LITE.hex,
            "Advocate = PKLite = Pink"
        );
    }

    /// Blip colors follow the rule's branch order.
    #[test]
    fn get_blip_color_follows_the_documented_decision_order() {
        // No object at all.
        assert_eq!(get_blip_color(None).hex, semantic::DEFAULT.hex);
        // The hidden bit beats everything, including an explicit blip colour.
        let mut hidden = obj(bitfield::HIDDEN | bitfield::PORTAL);
        hidden.blip_color = 5;
        assert_eq!(get_blip_color(Some(&hidden)).hex, semantic::DEFAULT.hex);

        // An explicit `blip_color` beats the bitfield.
        for (v, want) in [
            (1u8, BLUE),
            (2, GOLD),
            (3, WHITE),
            (4, PURPLE),
            (5, RED),
            (6, PINK),
            (7, GREEN),
            (8, YELLOW),
            (9, CYAN),
            (10, BRIGHT_GREEN),
        ] {
            let mut o = obj(bitfield::PORTAL);
            o.blip_color = v;
            assert_eq!(get_blip_color(Some(&o)).hex, want.hex, "blip_color {v}");
        }
        // An out-of-range `blip_color` falls to the default and does *not* fall through to the
        // bitfield tests.
        let mut o = obj(bitfield::PORTAL);
        o.blip_color = 99;
        assert_eq!(get_blip_color(Some(&o)).hex, semantic::DEFAULT.hex);

        // Portal before vendor, vendor before creature.
        assert_eq!(
            get_blip_color(Some(&obj(bitfield::PORTAL | bitfield::VENDOR))).hex,
            semantic::PORTAL.hex
        );
        assert_eq!(
            get_blip_color(Some(&obj(bitfield::VENDOR))).hex,
            semantic::VENDOR.hex
        );

        // Creature needs the bit *and* attackable *and* not a player.
        let mut c = obj(bitfield::CREATURE);
        c.is_attackable = true;
        assert_eq!(get_blip_color(Some(&c)).hex, semantic::CREATURE.hex);
        c.is_attackable = false;
        assert_eq!(get_blip_color(Some(&c)).hex, semantic::DEFAULT.hex);

        // A non-player that matched nothing is the default.
        assert_eq!(get_blip_color(Some(&obj(0))).hex, semantic::DEFAULT.hex);
    }

    /// Oracle: the player branch of the same rule — the admin test needs `0x100000` set and
    /// `0x40` clear, and the two fellowship tests run *after* the base is chosen and override it.
    #[test]
    fn the_player_branch_of_get_blip_color_layers_fellowship_over_the_base() {
        let player = |f: u32| {
            let mut o = obj(f);
            o.is_player = true;
            o
        };
        assert_eq!(get_blip_color(Some(&player(0))).hex, semantic::DEFAULT.hex);
        assert_eq!(
            get_blip_color(Some(&player(bitfield::ADMIN))).hex,
            semantic::ADMIN.hex
        );
        assert_eq!(
            get_blip_color(Some(&player(bitfield::ADMIN | bitfield::NOT_ADMIN))).hex,
            semantic::DEFAULT.hex,
            "0x40 suppresses the admin colour"
        );

        let mut pk = player(0);
        pk.is_pk = true;
        assert_eq!(get_blip_color(Some(&pk)).hex, semantic::PLAYER_KILLER.hex);
        let mut lite = player(0);
        lite.is_pk_lite = true;
        assert_eq!(get_blip_color(Some(&lite)).hex, semantic::PK_LITE.hex);
        assert_eq!(
            get_blip_color(Some(&player(bitfield::PLAYER_CREATURE))).hex,
            semantic::CREATURE.hex
        );

        // Fellowship overrides whatever base was chosen, leader first.
        let mut f = pk;
        f.is_fellow = true;
        assert_eq!(get_blip_color(Some(&f)).hex, semantic::FELLOWSHIP.hex);
        f.is_fellowship_leader = true;
        assert_eq!(
            get_blip_color(Some(&f)).hex,
            semantic::FELLOWSHIP_LEADER.hex
        );
    }

    /// Blip shapes follow the value table, including the
    /// "fellowship shape minus is-leader" arithmetic.
    #[test]
    fn get_blip_shape_follows_the_documented_decision_order_and_values() {
        assert_eq!(BlipShape::Undef as i32, 0);
        assert_eq!(BlipShape::Circle as i32, 1);
        assert_eq!(BlipShape::AllegianceMember as i32, 2);
        assert_eq!(BlipShape::Threat as i32, 3);
        assert_eq!(BlipShape::Default as i32, 4);
        assert_eq!(BlipShape::FellowshipLeader as i32, 5);
        assert_eq!(BlipShape::Fellowship as i32, 6);
        assert_eq!(BlipShape::ThreatAllegiance as i32, 7);
        assert_eq!(BlipShape::AllegianceMember.geometric(), "Box");
        assert_eq!(BlipShape::Default.geometric(), "Plus");
        // "Fellowship - isLeader": 6 - 1 = 5.
        assert_eq!(
            BlipShape::Fellowship as i32 - 1,
            BlipShape::FellowshipLeader as i32
        );

        assert_eq!(get_blip_shape(None, None), BlipShape::Undef);
        assert_eq!(
            get_blip_shape(Some(&obj(bitfield::HIDDEN)), None),
            BlipShape::Undef
        );

        let mut fellow = obj(0);
        fellow.is_fellow = true;
        assert_eq!(get_blip_shape(Some(&fellow), None), BlipShape::Fellowship);
        fellow.is_fellowship_leader = true;
        assert_eq!(
            get_blip_shape(Some(&fellow), None),
            BlipShape::FellowshipLeader
        );

        // The allegiance / threat tests need a player object to compare against.
        let mut ally = obj(0);
        ally.is_allegiance_member = true;
        assert_eq!(
            get_blip_shape(Some(&ally), None),
            BlipShape::Default,
            "no player, no test"
        );
        assert_eq!(
            get_blip_shape(Some(&ally), Some(&obj(0))),
            BlipShape::AllegianceMember
        );

        let mut pk = obj(0);
        pk.is_pk = true;
        let mut me = obj(0);
        me.is_pk = true;
        assert_eq!(get_blip_shape(Some(&pk), Some(&me)), BlipShape::Threat);
        assert_eq!(
            get_blip_shape(Some(&pk), Some(&obj(0))),
            BlipShape::Default,
            "a PK is only a Threat to a PK"
        );
        assert_eq!(
            get_blip_shape(Some(&obj(0)), Some(&obj(0))),
            BlipShape::Default
        );
    }

    /// the exact pixel offsets of all eight shapes, and the
    /// "every blip is at most 3×3" consequence.
    #[test]
    fn the_eight_blip_shapes_cover_the_documented_pixels() {
        let s = |sh: BlipShape| {
            let mut v = sh.pixels();
            v.sort_unstable();
            v
        };
        assert_eq!(s(BlipShape::Undef), Vec::<(i32, i32)>::new());
        assert_eq!(s(BlipShape::Circle), vec![(0, 0)]);
        // A cross = centre + the four edge neighbours.
        assert_eq!(
            s(BlipShape::Default),
            vec![(-1, 0), (0, -1), (0, 0), (0, 1), (1, 0)]
        );
        // An X = centre + the four corners.
        assert_eq!(
            s(BlipShape::Threat),
            vec![(-1, -1), (-1, 1), (0, 0), (1, -1), (1, 1)]
        );
        // A hollow box = edges + corners: a hollow 3x3 box, eight pixels, no centre.
        assert_eq!(s(BlipShape::AllegianceMember).len(), 8);
        assert!(!s(BlipShape::AllegianceMember).contains(&(0, 0)));
        // A filled box = the hollow box + the centre.
        assert_eq!(s(BlipShape::ThreatAllegiance).len(), 9);
        assert!(s(BlipShape::ThreatAllegiance).contains(&(0, 0)));
        assert_eq!(
            s(BlipShape::FellowshipLeader),
            vec![(-1, 1), (0, 0), (0, 1), (1, 1)]
        );
        assert_eq!(
            s(BlipShape::Fellowship),
            vec![(-1, -1), (0, -1), (0, 0), (1, -1)]
        );

        // "every blip is at most 3×3 pixels".
        for sh in [
            BlipShape::Circle,
            BlipShape::AllegianceMember,
            BlipShape::Threat,
            BlipShape::Default,
            BlipShape::FellowshipLeader,
            BlipShape::Fellowship,
            BlipShape::ThreatAllegiance,
        ] {
            for (x, y) in sh.pixels() {
                assert!((-1..=1).contains(&x) && (-1..=1).contains(&y), "{sh:?}");
            }
        }
    }
}
