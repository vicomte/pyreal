//! The barber -- the appearance edit opened by `0x0075 Character_StartBarber`: its private
//! character-generation state and preview, the appearance controls and the `0x0311` Apply request.

use std::rc::Rc;

use pyreal_primitives::DataId;

use crate::chargen::{Cg3dView, CharGenTables, EParts, PaletteSample, HUMAN_SETUP_ID};
use pyreal_chargen::CharGenState;
use pyreal_client_contract::view::{BarberAppearance, BarberView, GameView, UiRequest};

/// The string table the special option's caption tokens are rows of.
pub const CAPTION_STRING_TABLE: DataId = DataId(0x2300_0001);
const SHADOW_NO_CROWN_CAPTION: &str = "ID_Barber_Shadow_NoCrown";
const EMPYREAN_EARTHBOUND_CAPTION: &str = "ID_Barber_Empyrean_Earthbound";
const UNDEAD_NO_FLAME_CAPTION: &str = "ID_Barber_Undead_NoFlame";
const UMBRAEN_MALE_CROWN_SETUP: DataId = DataId(0x0200_196F);
const UMBRAEN_MALE_NO_CROWN_SETUP: DataId = DataId(0x0200_1A5F);
const UMBRAEN_FEMALE_CROWN_SETUP: DataId = DataId(0x0200_1970);
const UMBRAEN_FEMALE_NO_CROWN_SETUP: DataId = DataId(0x0200_1A5E);
const UMBRAEN_MALE_CROWN_EFFECT: DataId = DataId(0x3300_1256);
const UMBRAEN_MALE_NO_CROWN_EFFECT: DataId = DataId(0x3300_12BB);
const UMBRAEN_FEMALE_CROWN_EFFECT: DataId = DataId(0x3300_1257);
const UMBRAEN_FEMALE_NO_CROWN_EFFECT: DataId = DataId(0x3300_12BA);
const PENUMBRAEN_MALE_CROWN_SETUP: DataId = DataId(0x0200_196E);
const PENUMBRAEN_MALE_NO_CROWN_SETUP: DataId = DataId(0x0200_1A5D);
const PENUMBRAEN_FEMALE_CROWN_SETUP: DataId = DataId(0x0200_196D);
const PENUMBRAEN_FEMALE_NO_CROWN_SETUP: DataId = DataId(0x0200_1A5C);
const PENUMBRAEN_MALE_CROWN_EFFECT: DataId = DataId(0x3300_1255);
const PENUMBRAEN_FEMALE_CROWN_EFFECT: DataId = DataId(0x3300_1254);
const EMPYREAN_MALE_FLOAT_MOTION: DataId = DataId(0x0900_020B);
const EMPYREAN_FEMALE_FLOAT_MOTION: DataId = DataId(0x0900_020A);
const EMPYREAN_MALE_EARTHBOUND_MOTION: DataId = DataId(0x0900_020E);
const EMPYREAN_FEMALE_EARTHBOUND_MOTION: DataId = DataId(0x0900_020D);
const UNDEAD_MALE_SKELETON_SETUP: DataId = DataId(0x0200_1A9C);
const UNDEAD_MALE_SKELETON_NO_FLAME_SETUP: DataId = DataId(0x0200_1A9E);
const UNDEAD_MALE_ZOMBIE_SETUP: DataId = DataId(0x0200_1A9D);
const UNDEAD_MALE_ZOMBIE_NO_FLAME_SETUP: DataId = DataId(0x0200_1A96);
const UNDEAD_FEMALE_SKELETON_SETUP: DataId = DataId(0x0200_1AA0);
const UNDEAD_FEMALE_SKELETON_NO_FLAME_SETUP: DataId = DataId(0x0200_1A9F);
const UNDEAD_FEMALE_ZOMBIE_SETUP: DataId = DataId(0x0200_1AA1);
const UNDEAD_FEMALE_ZOMBIE_NO_FLAME_SETUP: DataId = DataId(0x0200_1AA2);
const UNDEAD_MALE_SKELETON_EFFECT: DataId = DataId(0x3300_12D4);
const UNDEAD_MALE_ZOMBIE_EFFECT: DataId = DataId(0x3300_12D3);
const UNDEAD_FEMALE_SKELETON_EFFECT: DataId = DataId(0x3300_12D5);
const UNDEAD_FEMALE_ZOMBIE_EFFECT: DataId = DataId(0x3300_12D6);

/// The barber's private appearance state and preview.
#[derive(Debug, Default)]
pub struct Barber {
    tables: Option<Rc<CharGenTables>>,
    /// The private character-generation state initialized from the notice.
    pub state: CharGenState,
    /// The private preview view consumed by the host renderer.
    pub view3d: Cg3dView,
    /// Last notice edge adopted. Kept after Cancel so the pull seam cannot reopen it.
    pub active_generation: Option<u64>,
    /// Accepted appearance-arrow gestures, an explicit input denominator.
    pub edits: u64,
    /// Accepted Cancel gestures.
    pub cancels: u64,
    /// Accepted Apply gestures, including native generation failures (which still close).
    pub applies: u64,
    /// Native option-one state for heritages 5 and 10. Their wire option remains literal zero;
    /// this chooses the sex-specific crown/no-crown setup and local transformation script.
    no_crown: bool,
    /// Native Empyrean option one: Earthbound when checked, floating when unchecked.
    empyrean_earthbound: bool,
    /// Native Undead option one: suppress the setup's flame particles when checked.
    undead_no_flame: bool,
}

impl Barber {
    /// Supply the same two dat tables character generation consumes.
    pub fn set_tables(&mut self, tables: Rc<CharGenTables>) {
        self.tables = Some(tables);
    }

    /// The host-side preview dresser needs the same table handle as character generation.
    #[must_use]
    pub fn tables(&self) -> Option<Rc<CharGenTables>> {
        self.tables.clone()
    }

    /// Take a new Start notice, if one has arrived since the last: the appearance state is
    /// initialized from it. True when one was taken. Any interface's barber starts here.
    pub fn adopt(&mut self, view: &dyn GameView) -> bool {
        let Some(start) = view.barber() else {
            return false;
        };
        if self.active_generation == Some(start.generation) {
            return false;
        }
        self.active_generation = Some(start.generation);
        self.initialize(start);
        true
    }

    /// The special option the heritage has, if any: its caption's token in
    /// [`CAPTION_STRING_TABLE`], and whether it is on. Empyrean's Earthbound, the Undead's
    /// no-flame and the shadow heritages' no-crown.
    #[must_use]
    pub fn special_option(&self) -> Option<(&'static str, bool)> {
        match self.state.heritage_group {
            9 => Some((EMPYREAN_EARTHBOUND_CAPTION, self.empyrean_earthbound)),
            11 => Some((UNDEAD_NO_FLAME_CAPTION, self.undead_no_flame)),
            5 | 10 => Some((SHADOW_NO_CROWN_CAPTION, self.no_crown)),
            _ => None,
        }
    }

    /// Turn the special option on or off. False when the heritage has none.
    pub fn set_special_option(&mut self, on: bool) -> bool {
        match self.state.heritage_group {
            9 => self.empyrean_earthbound = on,
            11 => {
                self.undead_no_flame = on;
                self.update_special_setup();
            }
            5 | 10 => {
                self.no_crown = on;
                self.update_special_setup();
            }
            _ => return false,
        }
        self.edits += 1;
        true
    }

    /// The Apply gesture: the local preludes and the finish-barber request, in the order the
    /// client sends them. Retail closes even when the appearance cannot be generated; only the
    /// successful arm carries the finish request.
    pub fn apply(&mut self) -> Vec<UiRequest> {
        let mut out = Vec::new();
        // Empyrean installs its selected motion table; heritages 5 and 10 replace local
        // particles, including the zero-PES arm. These are native local preludes; the
        // authoritative object-description update owns the lasting appearance.
        if self.state.heritage_group == 9 {
            out.push(UiRequest::BarberLocalMotionTable(
                self.empyrean_motion_table(),
            ));
        }
        if self.state.heritage_group == 11 {
            if let Some((_, effect)) = self.undead_setup_and_effect() {
                out.push(UiRequest::BarberLocalEffect(effect));
            }
        }
        if matches!(self.state.heritage_group, 5 | 10) {
            if let Some((_, effect)) = self.special_setup_and_effect() {
                out.push(UiRequest::BarberLocalEffect(effect));
            }
        }
        if let Some(appearance) = self.finish_appearance() {
            out.push(UiRequest::BarberFinish(appearance));
        }
        self.applies += 1;
        out
    }

    /// The Cancel gesture: nothing is sent.
    pub fn cancel(&mut self) {
        self.cancels += 1;
    }

    /// The choice `part` shows now and how many it has: hair, eyes, nose or mouth.
    #[must_use]
    pub fn choice(&self, part: EParts) -> Option<(i32, usize)> {
        let tables = self.tables.as_ref()?;
        let sx = self.state.sex(&tables.chargen)?;
        Some(match part {
            EParts::Hair => (self.state.hair_style, sx.hair_styles.len()),
            EParts::Eyes => (self.state.eyes_strip, sx.eye_strips.len()),
            EParts::Nose => (self.state.nose_strip, sx.nose_strips.len()),
            EParts::Mouth => (self.state.mouth_strip, sx.mouth_strips.len()),
            _ => return None,
        })
    }

    /// The colours `part` offers, as the colour wheel shows them (`None` for one with no colour
    /// to show), and the one chosen: hair and eyes have their own; nose, mouth and skin share the
    /// skin's one.
    #[must_use]
    pub fn colors(&self, part: EParts) -> (Vec<Option<u32>>, Option<i32>) {
        let chosen = match part {
            EParts::Hair => Some(self.state.hair_color),
            EParts::Eyes => Some(self.state.eye_color),
            EParts::Nose | EParts::Mouth | EParts::Skin => Some(0),
            _ => None,
        };
        (self.swatches(part), chosen)
    }

    /// The shade `part` has, where it has one: hair its own, nose, mouth and skin the skin's.
    #[must_use]
    pub fn shade(&self, part: EParts) -> Option<f64> {
        match part {
            EParts::Hair => Some(self.state.hair_shade),
            EParts::Nose | EParts::Mouth | EParts::Skin => Some(self.state.skin_shade),
            _ => None,
        }
    }

    /// Choose colour `color` of `part`'s family. False when it has no such colour.
    pub fn choose_color(&mut self, part: EParts, color: i32) -> bool {
        if !self.apply_color(Some(part), color) {
            return false;
        }
        self.edits += 1;
        true
    }

    /// Choose shade `shade` (0 to 1) of `part`'s family. False when it has no shade.
    pub fn choose_shade(&mut self, part: EParts, shade: f64) -> bool {
        if !self.apply_shade(Some(part), shade) {
            return false;
        }
        self.edits += 1;
        true
    }

    /// Step `part`'s choice by `delta`, wrapping. False when the part has no choices.
    pub fn step(&mut self, part: EParts, delta: i32) -> bool {
        if !self.cycle(part, delta) {
            return false;
        }
        self.edits += 1;
        true
    }

    /// The colour `color` of `part`'s family, written to the appearance state; false when the
    /// family has no such colour. Only hair and eyes own palette-list choices.
    fn apply_color(&mut self, part: Option<EParts>, color: i32) -> bool {
        let Some(tables) = self.tables.clone() else {
            return false;
        };
        let Some(sx) = self.state.sex(&tables.chargen) else {
            return false;
        };
        let count = match part {
            Some(EParts::Hair) => sx.hair_colors.len(),
            Some(EParts::Eyes) => sx.eye_colors.len(),
            Some(EParts::Nose | EParts::Mouth | EParts::Skin) => 1,
            _ => return false,
        };
        let Some(index) = usize::try_from(color).ok().filter(|i| *i < count) else {
            return false;
        };
        match part {
            Some(EParts::Hair) => self.state.set_hair_color(i32::try_from(index).unwrap_or(0)),
            Some(EParts::Eyes) => self.state.set_eye_color(i32::try_from(index).unwrap_or(0)),
            _ => {}
        }
        true
    }

    /// The shade written to the appearance state: hair has its own, nose, mouth and skin share
    /// the skin's, and eyes have none.
    fn apply_shade(&mut self, part: Option<EParts>, shade: f64) -> bool {
        let shade = shade.clamp(0.0, 1.0);
        match part {
            Some(EParts::Hair) => self.state.set_hair_shade(shade),
            Some(EParts::Nose | EParts::Mouth | EParts::Skin) => self.state.set_skin_shade(shade),
            _ => return false,
        }
        true
    }

    fn initialize(&mut self, start: BarberView) {
        self.state = CharGenState::default();
        let setup = DataId(start.setup_id);
        self.no_crown = match start.heritage {
            5 => matches!(
                setup,
                UMBRAEN_MALE_NO_CROWN_SETUP | UMBRAEN_FEMALE_NO_CROWN_SETUP
            ),
            10 => {
                matches!(
                    setup,
                    PENUMBRAEN_MALE_NO_CROWN_SETUP | PENUMBRAEN_FEMALE_NO_CROWN_SETUP
                )
            }
            _ => false,
        };
        // Page initialisation reads argument 15, the incoming `option1`, not argument 14's
        // setup ID.
        self.empyrean_earthbound = start.heritage == 9 && start.option1 != 0;
        self.undead_no_flame = start.heritage == 11
            && matches!(
                setup,
                UNDEAD_MALE_SKELETON_NO_FLAME_SETUP
                    | UNDEAD_MALE_ZOMBIE_NO_FLAME_SETUP
                    | UNDEAD_FEMALE_SKELETON_NO_FLAME_SETUP
                    | UNDEAD_FEMALE_ZOMBIE_NO_FLAME_SETUP
            );
        let Some(tables) = self.tables.clone() else {
            self.state.heritage_group = start.heritage;
            self.state.gender = start.gender;
            self.state.setup_id = DataId(start.setup_id);
            self.initialize_view();
            return;
        };

        // Native initializes the shared char-gen state with a full random character first, then
        // clears headgear and replaces every appearance value represented by this notice.
        self.state
            .randomize_character(&tables.chargen, &tables.skills, true);
        self.state.set_headgear_style(&tables.chargen, -1);
        self.state.set_gender(&tables.chargen, start.gender);
        self.state
            .set_heritage_group(&tables.chargen, &tables.skills, start.heritage);

        if let Some(sx) = self.state.sex(&tables.chargen) {
            self.state.hair_style = find_hair(sx, start.head_object).unwrap_or(0);
            self.state.eyes_strip = find_eyes(sx, start.eyes_texture).unwrap_or(0);
            self.state.nose_strip = find_strip(&sx.nose_strips, start.nose_texture).unwrap_or(0);
            self.state.mouth_strip = find_strip(&sx.mouth_strips, start.mouth_texture).unwrap_or(0);
            self.state.eye_color = sx
                .eye_colors
                .iter()
                .position(|id| *id == start.eyes_palette)
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(0);
            if let Some(colors) = tables.colors.as_ref() {
                self.state.skin_shade = inverse_shade(
                    colors.pal_set_palettes(sx.skin_palset).as_deref(),
                    start.skin_palette,
                )
                .unwrap_or(0.0);
                let (hair_color, hair_shade) = sx
                    .hair_colors
                    .iter()
                    .enumerate()
                    .find_map(|(color, set)| {
                        inverse_shade(
                            colors.pal_set_palettes(DataId(*set)).as_deref(),
                            start.hair_palette,
                        )
                        .and_then(|shade| i32::try_from(color).ok().map(|color| (color, shade)))
                    })
                    .unwrap_or((0, 0.0));
                self.state.hair_color = hair_color;
                self.state.hair_shade = hair_shade;
            }
        }
        self.initialize_view();
        let setup = self.state.get_setup_id(&tables.chargen);
        self.view3d.setup = if setup.0 == 0 { HUMAN_SETUP_ID } else { setup };
    }

    fn initialize_view(&mut self) {
        self.view3d = Cg3dView::default();
        self.view3d.initialize(&self.state);
        // Page initialisation: the initial camera, with the closer special-
        // creature position used by Gear Knight and both Olthoi heritages.
        self.view3d.camera_position = match self.state.heritage_group {
            6 | 12 | 13 => [0.0, -0.8, 1.5],
            _ => [0.0, -0.65, 1.7],
        };
        self.view3d.camera_direction = [0.0, 0.0, 0.0];
        self.view3d.zoomed_in = true;
        self.view3d.animating = false;
        self.view3d.bg_setup = DataId(0);
    }

    /// Rotate the private preview on the same global tick as character generation.
    pub fn tick_preview(&mut self, dt: f64) {
        self.view3d.do_rotation(dt);
    }

    /// The colour wheel's colours for `part`'s family, every one of them.
    fn swatches(&self, part: EParts) -> Vec<Option<u32>> {
        let Some(tables) = self.tables.clone() else {
            return Vec::new();
        };
        let Some(colors) = tables.colors.clone() else {
            return Vec::new();
        };
        let Some(sx) = self.state.sex(&tables.chargen) else {
            return Vec::new();
        };
        match part {
            EParts::Hair => sx
                .hair_colors
                .iter()
                .map(|id| colors.pal_set_color(DataId(*id), PaletteSample::Hair))
                .collect(),
            EParts::Eyes => sx
                .eye_colors
                .iter()
                .map(|id| colors.palette_color(DataId(*id), PaletteSample::Eyes))
                .collect(),
            EParts::Nose | EParts::Mouth | EParts::Skin => {
                vec![colors.pal_set_color(sx.skin_palset, PaletteSample::Skin)]
            }
            EParts::Invalid
            | EParts::Headgear
            | EParts::Shirt
            | EParts::Trousers
            | EParts::Footwear => Vec::new(),
        }
    }

    /// The char-gen base-appearance generation, reduced to the sixteen fields
    /// the finish-barber request consumes. The object-description merge remains the preview dresser's
    /// job; this extracts the same part/texture changes and concrete palette IDs from its inputs.
    fn finish_appearance(&self) -> Option<BarberAppearance> {
        let tables = self.tables.as_ref()?;
        let sx = self.state.sex(&tables.chargen)?;
        let pick = |i: i32, n: usize| usize::try_from(i).ok().filter(|i| *i < n);
        let hair = &sx.hair_styles[pick(self.state.hair_style, sx.hair_styles.len())?];
        let eyes = &sx.eye_strips[pick(self.state.eyes_strip, sx.eye_strips.len())?];
        let nose = &sx.nose_strips[pick(self.state.nose_strip, sx.nose_strips.len())?].1;
        let mouth = &sx.mouth_strips[pick(self.state.mouth_strip, sx.mouth_strips.len())?].1;

        let head_object = if self.state.heritage_group == 6 {
            0
        } else {
            hair.objdesc
                .anim_part_changes
                .first()
                .map_or(0, |(_, id)| id.0)
        };
        // Native omits the hair texture pair entirely for Gear Knight and both Olthoi heritages.
        let head = if matches!(self.state.heritage_group, 6 | 12 | 13) {
            (0, 0)
        } else {
            texture_pair(&hair.objdesc, 0)
        };
        let eyes_desc = if hair.bald == 0 {
            &eyes.objdesc
        } else {
            &eyes.objdesc_bald
        };
        let eyes = texture_pair(eyes_desc, 0);
        let nose = texture_pair(nose, 0);
        let mouth = texture_pair(mouth, 0);

        let colors = tables.colors.as_ref()?;
        let skin_palette = palette_at_shade(
            colors.pal_set_palettes(sx.skin_palset)?.as_slice(),
            self.state.skin_shade,
        )?;
        let hair_color = pick(self.state.hair_color, sx.hair_colors.len())?;
        let hair_palette = palette_at_shade(
            colors
                .pal_set_palettes(DataId(sx.hair_colors[hair_color]))?
                .as_slice(),
            self.state.hair_shade,
        )?;
        let eyes_palette = sx.eye_colors[pick(self.state.eye_color, sx.eye_colors.len())?];
        let setup = match self.state.heritage_group {
            5 | 10 => self.special_setup_and_effect()?.0,
            11 => self.undead_setup_and_effect()?.0,
            _ => self.state.get_setup_id(&tables.chargen),
        };

        Some(BarberAppearance {
            base_palette: sx.base_palette.0,
            head_object,
            head_texture: head.1,
            default_head_texture: head.0,
            eyes_texture: eyes.1,
            default_eyes_texture: eyes.0,
            nose_texture: nose.1,
            default_nose_texture: nose.0,
            mouth_texture: mouth.1,
            default_mouth_texture: mouth.0,
            skin_palette,
            hair_palette,
            eyes_palette,
            setup_id: setup.0,
            option1: i32::from(self.state.heritage_group == 9 && self.empyrean_earthbound),
            option2: 0,
        })
    }

    fn special_setup_and_effect(&self) -> Option<(DataId, DataId)> {
        let female = self.state.gender == 2;
        let pair = match (self.state.heritage_group, female, self.no_crown) {
            (5, false, false) => (UMBRAEN_MALE_CROWN_SETUP, UMBRAEN_MALE_CROWN_EFFECT),
            (5, false, true) => (UMBRAEN_MALE_NO_CROWN_SETUP, UMBRAEN_MALE_NO_CROWN_EFFECT),
            (5, true, false) => (UMBRAEN_FEMALE_CROWN_SETUP, UMBRAEN_FEMALE_CROWN_EFFECT),
            (5, true, true) => (
                UMBRAEN_FEMALE_NO_CROWN_SETUP,
                UMBRAEN_FEMALE_NO_CROWN_EFFECT,
            ),
            (10, false, false) => (PENUMBRAEN_MALE_CROWN_SETUP, PENUMBRAEN_MALE_CROWN_EFFECT),
            (10, false, true) => (PENUMBRAEN_MALE_NO_CROWN_SETUP, DataId(0)),
            (10, true, false) => (
                PENUMBRAEN_FEMALE_CROWN_SETUP,
                PENUMBRAEN_FEMALE_CROWN_EFFECT,
            ),
            (10, true, true) => (PENUMBRAEN_FEMALE_NO_CROWN_SETUP, DataId(0)),
            _ => return None,
        };
        Some(pair)
    }

    fn update_special_setup(&mut self) {
        let special = if self.state.heritage_group == 11 {
            self.undead_setup_and_effect()
        } else {
            self.special_setup_and_effect()
        };
        if let Some((setup, _)) = special {
            self.view3d.setup = setup;
        }
    }

    fn undead_setup_and_effect(&self) -> Option<(DataId, DataId)> {
        let tables = self.tables.as_ref()?;
        let generated = self.state.get_setup_id(&tables.chargen);
        // The setup dispatch recognizes the four zombie IDs; every other generated setup takes
        // the skeleton arm. There is no corresponding known-skeleton guard.
        let zombie = matches!(
            generated,
            UNDEAD_MALE_ZOMBIE_SETUP
                | UNDEAD_MALE_ZOMBIE_NO_FLAME_SETUP
                | UNDEAD_FEMALE_ZOMBIE_SETUP
                | UNDEAD_FEMALE_ZOMBIE_NO_FLAME_SETUP
        );
        let female = self.state.gender == 2;
        Some(match (female, zombie, self.undead_no_flame) {
            (false, false, false) => (UNDEAD_MALE_SKELETON_SETUP, UNDEAD_MALE_SKELETON_EFFECT),
            (false, true, false) => (UNDEAD_MALE_ZOMBIE_SETUP, UNDEAD_MALE_ZOMBIE_EFFECT),
            (true, false, false) => (UNDEAD_FEMALE_SKELETON_SETUP, UNDEAD_FEMALE_SKELETON_EFFECT),
            (true, true, false) => (UNDEAD_FEMALE_ZOMBIE_SETUP, UNDEAD_FEMALE_ZOMBIE_EFFECT),
            (false, false, true) => (UNDEAD_MALE_SKELETON_NO_FLAME_SETUP, DataId(0)),
            (false, true, true) => (UNDEAD_MALE_ZOMBIE_NO_FLAME_SETUP, DataId(0)),
            (true, false, true) => (UNDEAD_FEMALE_SKELETON_NO_FLAME_SETUP, DataId(0)),
            (true, true, true) => (UNDEAD_FEMALE_ZOMBIE_NO_FLAME_SETUP, DataId(0)),
        })
    }

    fn empyrean_motion_table(&self) -> DataId {
        match (self.state.gender == 2, self.empyrean_earthbound) {
            (false, false) => EMPYREAN_MALE_FLOAT_MOTION,
            (true, false) => EMPYREAN_FEMALE_FLOAT_MOTION,
            (false, true) => EMPYREAN_MALE_EARTHBOUND_MOTION,
            (true, true) => EMPYREAN_FEMALE_EARTHBOUND_MOTION,
        }
    }

    fn cycle(&mut self, part: EParts, delta: i32) -> bool {
        let Some(tables) = self.tables.clone() else {
            return false;
        };
        let Some(sx) = self.state.sex(&tables.chargen) else {
            return false;
        };
        let (current, count) = match part {
            EParts::Hair => (self.state.hair_style, sx.hair_styles.len()),
            EParts::Eyes => (self.state.eyes_strip, sx.eye_strips.len()),
            EParts::Nose => (self.state.nose_strip, sx.nose_strips.len()),
            EParts::Mouth => (self.state.mouth_strip, sx.mouth_strips.len()),
            EParts::Skin
            | EParts::Invalid
            | EParts::Headgear
            | EParts::Shirt
            | EParts::Trousers
            | EParts::Footwear => return false,
        };
        let Ok(count) = i32::try_from(count) else {
            return false;
        };
        if count == 0 {
            return false;
        }
        let next = (current + delta).rem_euclid(count);
        match part {
            EParts::Hair => self.state.hair_style = next,
            EParts::Eyes => self.state.eyes_strip = next,
            EParts::Nose => self.state.nose_strip = next,
            EParts::Mouth => self.state.mouth_strip = next,
            _ => return false,
        }
        match self.state.heritage_group {
            // The barber panel leaves the special alternate setup latched across Hair/Next. The hair
            // choice changes, but neither that common case nor selection writes
            // the alternate setup id; option gestures and Apply remain the explicit remap points.
            5 | 10 | 11 => {}
            _ => {
                let setup = self.state.get_setup_id(&tables.chargen);
                self.view3d.setup = if setup.0 == 0 { HUMAN_SETUP_ID } else { setup };
            }
        }
        true
    }
}

fn texture_pair(desc: &pyreal_assets::tables::ObjDesc, index: usize) -> (u32, u32) {
    desc.texture_changes
        .get(index)
        .map_or((0, 0), |(_, old, new)| (old.0, new.0))
}

fn palette_at_shade(palettes: &[DataId], shade: f64) -> Option<u32> {
    if palettes.is_empty() || !(0.0..=1.0).contains(&shade) {
        return None;
    }
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let index = ((palettes.len() as f64 - 0.000_001) * shade) as usize;
    palettes.get(index).map(|id| id.0)
}

fn find_hair(sx: &pyreal_assets::tables::SexCg, id: u32) -> Option<i32> {
    sx.hair_styles
        .iter()
        .position(|h| {
            h.objdesc
                .anim_part_changes
                .first()
                .is_some_and(|(_, part)| part.0 == id)
        })
        .and_then(|i| i32::try_from(i).ok())
}

fn find_eyes(sx: &pyreal_assets::tables::SexCg, id: u32) -> Option<i32> {
    sx.eye_strips
        .iter()
        .position(|e| {
            [&e.objdesc, &e.objdesc_bald]
                .into_iter()
                .any(|d| d.texture_changes.iter().any(|(_, _, new)| new.0 == id))
        })
        .and_then(|i| i32::try_from(i).ok())
}

fn find_strip(strips: &[(u32, pyreal_assets::tables::ObjDesc)], id: u32) -> Option<i32> {
    strips
        .iter()
        .position(|(_, d)| d.texture_changes.iter().any(|(_, _, new)| new.0 == id))
        .and_then(|i| i32::try_from(i).ok())
}

fn inverse_shade(palettes: Option<&[DataId]>, palette: u32) -> Option<f64> {
    let palettes = palettes?;
    let index = palettes.iter().position(|id| id.0 == palette)?;
    let numerator = u32::try_from(index).ok()?.checked_add(1)?;
    let denominator = u32::try_from(palettes.len()).ok().filter(|n| *n != 0)?;
    Some(f64::from(numerator) / f64::from(denominator))
}
