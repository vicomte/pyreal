//! The presentation options from another era: which era's ground, which era's sky, and which
//! era's look the world's objects draw with.
//!
//! These are this client's own options, not retail ones. A dat set from before Throne of Destiny
//! carries two regions, one its clients loaded when they drew in software and one they loaded
//! when they drew with 3D hardware; the later dat sets carry one. The three differ in their land
//! surface (how the ground is textured) and their sky, and any of the three can draw any world:
//! the cells give heights, a terrain type and road bits per vertex, numbered the same in every
//! era, and the region decides only how they look.
//!
//! The objects are the third: every object is drawn by its setup (the parts, how they are joined
//! and how the world's motion data moves them), which stays the world's, and each part's model,
//! surfaces, pictures and palettes, which can be the other era's. [`OBJECTS`] chooses: the
//! files from before Throne of Destiny (Classic) or the later ones (Modern).
//!
//! No player chooses any more: every world draws with its own look (Modern on every later world;
//! a world from before Throne of Destiny keeps its own region and objects, which its dats need).
//! An older profile's [`GROUND`], [`SKY`] and [`OBJECTS`] are read as nothing; `--object-visuals`
//! still draws the objects with another era's look for a developer.

/// `[Render] Ground`: the land surface the ground is drawn with.
pub const GROUND: &str = crate::options::names::GROUND;
/// `[Render] Sky`: the sky, and the light and fog that come with it.
pub const SKY: &str = crate::options::names::SKY;
/// `[Render] Objects`: the era whose models, surfaces, pictures and palettes the world's objects
/// are drawn with.
pub const OBJECTS: &str = crate::options::names::OBJECTS;

/// The value all three preferences hold for "the world's own": the hardware region of a world from
/// before Throne of Destiny (Classic Blend and its sky), the one region of a later world (Modern).
pub const WORLD_DEFAULT: i32 = 0;

/// One of the three regions a ground or a sky can be drawn from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RegionStyle {
    /// The region the clients before Throne of Destiny loaded when they drew in software. Its
    /// ground recolours a few shared pictures with each corner's terrain palette (palette
    /// shifting).
    LegacySoftware,
    /// The region the clients before Throne of Destiny loaded when they drew with 3D hardware.
    /// Its ground blends terrain textures through alpha masks (texture merging), at a 128-pixel
    /// base.
    LegacyHardware,
    /// The region of the end-of-retail dat set, whose ground texture-merges at a 512-pixel base.
    Late,
}

impl RegionStyle {
    /// The three, in the order the options list them.
    pub const ALL: [Self; 3] = [Self::LegacySoftware, Self::LegacyHardware, Self::Late];

    /// The preference value.
    #[must_use]
    pub const fn value(self) -> i32 {
        match self {
            Self::LegacySoftware => 1,
            Self::LegacyHardware => 2,
            Self::Late => 3,
        }
    }

    /// The style a preference value names; `None` for [`WORLD_DEFAULT`] and anything else.
    #[must_use]
    pub const fn from_value(v: i32) -> Option<Self> {
        match v {
            1 => Some(Self::LegacySoftware),
            2 => Some(Self::LegacyHardware),
            3 => Some(Self::Late),
            _ => None,
        }
    }

    /// The ground's name for it, as the options list it.
    #[must_use]
    pub const fn ground_label(self) -> &'static str {
        match self {
            Self::LegacySoftware => "Palette Shift",
            Self::LegacyHardware => "Classic Blend",
            Self::Late => "Modern Blend",
        }
    }

    /// The sky's name for it, as the options list it.
    #[must_use]
    pub const fn sky_label(self) -> &'static str {
        match self {
            Self::LegacySoftware => "Classic Software",
            Self::LegacyHardware => "Classic Hardware",
            Self::Late => "Modern",
        }
    }

    /// The files it is read from.
    #[must_use]
    pub const fn required_files(self) -> RequiredFiles {
        match self {
            Self::LegacySoftware | Self::LegacyHardware => RequiredFiles::Classic,
            Self::Late => RequiredFiles::Modern,
        }
    }
}

/// Which dat set a style needs: the one from before Throne of Destiny, or a later one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequiredFiles {
    /// A `portal.dat` from before Throne of Destiny: the world's own, or one given for
    /// presentation beside a later world.
    Classic,
    /// The end-of-retail files: the world's own, or the interface files beside an older world.
    Modern,
}

impl RequiredFiles {
    /// What the client shows when a ground style needing these files is chosen without them.
    #[must_use]
    pub const fn ground_notice(self) -> &'static str {
        match self {
            Self::Classic => "This terrain mode requires classic DATs",
            Self::Modern => "This terrain mode requires end-of-retail DATs",
        }
    }

    /// What the client shows when a sky needing these files is chosen without them.
    #[must_use]
    pub const fn sky_notice(self) -> &'static str {
        match self {
            Self::Classic => "This sky requires classic DATs",
            Self::Modern => "This sky requires end-of-retail DATs",
        }
    }

    /// What the client shows when an object mode needing these files is chosen without them.
    #[must_use]
    pub const fn objects_notice(self) -> &'static str {
        match self {
            Self::Classic => "This object mode requires classic DATs",
            Self::Modern => "This object mode requires end-of-retail DATs",
        }
    }
}

/// Which of the three preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Landscape {
    /// [`GROUND`].
    Ground,
    /// [`SKY`].
    Sky,
    /// [`OBJECTS`].
    Objects,
}

impl Landscape {
    /// The preference this is, by name, in any case.
    #[must_use]
    pub fn of(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case(GROUND) {
            Some(Self::Ground)
        } else if name.eq_ignore_ascii_case(SKY) {
            Some(Self::Sky)
        } else if name.eq_ignore_ascii_case(OBJECTS) {
            Some(Self::Objects)
        } else {
            None
        }
    }

    /// The preference name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ground => GROUND,
            Self::Sky => SKY,
            Self::Objects => OBJECTS,
        }
    }

    /// The styles this preference offers, in the order the options list them. The objects have
    /// one older look (both older regions name the same objects), stored as
    /// [`RegionStyle::LegacyHardware`].
    #[must_use]
    pub const fn styles(self) -> &'static [RegionStyle] {
        match self {
            Self::Ground | Self::Sky => &RegionStyle::ALL,
            Self::Objects => &[RegionStyle::LegacyHardware, RegionStyle::Late],
        }
    }

    /// The row's caption on an options page.
    #[must_use]
    pub const fn caption(self) -> &'static str {
        match self {
            Self::Ground => "Terrain Mode",
            Self::Sky => "Sky Mode",
            Self::Objects => "Object Mode",
        }
    }

    /// A style's name for this preference.
    #[must_use]
    pub const fn label(self, style: RegionStyle) -> &'static str {
        match self {
            Self::Ground => style.ground_label(),
            Self::Sky => style.sky_label(),
            Self::Objects => match style {
                RegionStyle::LegacySoftware | RegionStyle::LegacyHardware => "Classic",
                RegionStyle::Late => "Modern",
            },
        }
    }

    /// The notice for a refused style.
    #[must_use]
    pub const fn notice(self, files: RequiredFiles) -> &'static str {
        match self {
            Self::Ground => files.ground_notice(),
            Self::Sky => files.sky_notice(),
            Self::Objects => files.objects_notice(),
        }
    }

    /// How the preferences file spells a value: one word per choice.
    #[must_use]
    pub const fn file_word(self, style: Option<RegionStyle>) -> &'static str {
        match (self, style) {
            (_, None) => "World",
            (Self::Ground, Some(RegionStyle::LegacySoftware)) => "PaletteShift",
            (Self::Ground, Some(RegionStyle::LegacyHardware)) => "ClassicBlend",
            (Self::Ground, Some(RegionStyle::Late)) => "ModernBlend",
            (Self::Sky, Some(RegionStyle::LegacySoftware)) => "ClassicSoftware",
            (Self::Sky, Some(RegionStyle::LegacyHardware)) => "ClassicHardware",
            (Self::Sky, Some(RegionStyle::Late)) => "Modern",
            (Self::Objects, Some(RegionStyle::LegacySoftware | RegionStyle::LegacyHardware)) => {
                "Classic"
            }
            (Self::Objects, Some(RegionStyle::Late)) => "Modern",
        }
    }
}

/// A preferences-file value: `Some(None)` is the world's own, `Some(Some(style))` a style, and
/// `None` a value that names neither (which leaves the preference where it was).
///
/// Every spelling of any of the three preferences is read for all three, in any case and with or
/// without spaces: the file words, the option labels, the words and labels this client used
/// before the older styles were named Classic (`LegacyBlend`, `Legacy Software`, `Legacy` and the
/// rest), and the three it wrote before the styles had names. `classic` (and `legacy`) is the
/// older hardware region (the objects' one older look). Of the three, `software` was the
/// palette-shift region and `later` the end-of-retail ground, and `hardware` was the world's own
/// hardware region, which is the world's default.
#[must_use]
pub fn parse(raw: &str) -> Option<Option<RegionStyle>> {
    let word: String = raw
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    Some(match word.as_str() {
        "world" | "worlddefault" | "default" | "hardware" => None,
        "paletteshift" | "classicsoftware" | "legacysoftware" | "software" => {
            Some(RegionStyle::LegacySoftware)
        }
        "classicblend" | "classichardware" | "classic" | "legacyblend" | "legacyhardware"
        | "legacy" => Some(RegionStyle::LegacyHardware),
        "modernblend" | "modern" | "later" => Some(RegionStyle::Late),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file words, the captions and the three older spellings all read; anything else does
    /// not.
    #[test]
    fn every_spelling_of_a_style_reads_and_the_older_three_keep_their_meaning() {
        for s in RegionStyle::ALL {
            for which in [Landscape::Ground, Landscape::Sky] {
                assert_eq!(parse(which.file_word(Some(s))), Some(Some(s)));
                assert_eq!(parse(which.label(s)), Some(Some(s)));
                assert_eq!(parse(&which.label(s).to_ascii_uppercase()), Some(Some(s)));
            }
        }
        assert_eq!(parse("software"), Some(Some(RegionStyle::LegacySoftware)));
        assert_eq!(parse("Later"), Some(Some(RegionStyle::Late)));
        assert_eq!(parse("hardware"), Some(None));
        assert_eq!(parse("World Default"), Some(None));
        assert_eq!(parse("tod"), None);
    }
}
