//! Character creation against the connected world's decoded tables.
use pyreal_assets::motion::ClothingTable;
use pyreal_assets::{CharGen, Decode, SkillTable};
use pyreal_chargen::{
    Attr, CharGenRng, CharGenState, CreationEntry, CreationPolicy, CreationRandom, CreationTables,
    SkillAdvancementClass,
};
use pyreal_dat::{ContainerEra, RetailDatStore};
use pyreal_primitives::DataId;
use std::{collections::BTreeMap, rc::Rc};

#[test]
fn the_tables_read_from_both_sets_of_files() {
    let classic = pyreal_dat::testing::open_classic_store_or_fail();
    let read = CreationTables::read(&classic).expect("the February 2005 tables");
    assert_eq!(read.chargen, tables(true).chargen);
    let modern = RetailDatStore::open_dir(&pyreal_dat::testing::dat_dir()).unwrap();
    assert!(CreationTables::read(&modern).is_some());
}

fn tables(old: bool) -> CreationTables {
    let store = if old {
        pyreal_dat::testing::open_classic_store_or_fail()
    } else {
        RetailDatStore::open_dir(&pyreal_dat::testing::dat_dir()).unwrap()
    };
    let era = if old {
        ContainerEra::Classic
    } else {
        ContainerEra::Modern
    };
    let decode = |id| store.read_portal(DataId(id)).unwrap();
    let chargen =
        CharGen::decode_payload_in(era, DataId(0x0e00_0002), &decode(0x0e00_0002)).unwrap();
    let skills =
        SkillTable::decode_payload_in(era, DataId(0x0e00_0004), &decode(0x0e00_0004)).unwrap();
    let mut clothing = BTreeMap::new();
    for sex in chargen
        .heritage_groups
        .values()
        .flat_map(|h| h.sexes.values())
    {
        for gear in sex
            .headgear
            .iter()
            .chain(&sex.shirts)
            .chain(&sex.pants)
            .chain(&sex.footwear)
        {
            clothing.entry(gear.clothing_table).or_insert_with(|| {
                ClothingTable::decode_payload_in(
                    era,
                    gear.clothing_table,
                    &decode(gear.clothing_table.0),
                )
                .unwrap()
            });
        }
    }
    CreationTables {
        chargen,
        skills,
        clothing: Rc::new(clothing),
    }
}

#[test]
fn classic_entry_keeps_nested_draws_and_summary_only_completes_missing_choices() {
    let t = tables(true);
    assert_eq!(t.heritage_keys(), [1, 2, 3]);
    assert_eq!(t.sex_keys(1), [2, 1]);
    for (entry, next) in [(CreationEntry::Normal, 2995), (CreationEntry::Quick, 19895)] {
        let mut state = CharGenState::with_policy(CreationPolicy::Classic);
        state.begin_creation(&t, entry, false);
        assert_eq!(
            (
                state.heritage_group,
                state.gender,
                state.template,
                state.start_area
            ),
            if entry == CreationEntry::Normal {
                (1, 2, 4, 0)
            } else {
                (1, 2, 1, 1)
            }
        );
        assert_eq!(
            [
                state.eyes_strip,
                state.nose_strip,
                state.mouth_strip,
                state.hair_color,
                state.eye_color,
                state.hair_style
            ],
            [10, 10, 9, 2, 1, 3]
        );
        assert_eq!(
            Attr::BALANCE_ORDER.map(|a| state.get(a)),
            if entry == CreationEntry::Normal {
                [50, 40, 10, 30, 100, 100]
            } else {
                [40, 30, 100, 100, 50, 10]
            }
        );
        assert_eq!(state.rng.crt.clone().next_u16(), next);
        assert_eq!(
            state.rng.ran2.clone().next_f64(),
            CharGenRng::default().ran2.next_f64(),
            "Classic uses only the CRT stream"
        );
        if entry == CreationEntry::Normal {
            assert_eq!(
                [
                    state.headgear_style,
                    state.shirt_style,
                    state.trousers_style,
                    state.footwear_style
                ],
                [-1; 4]
            );
            assert_eq!(state.frozen, [false; 3]);
        } else {
            assert_eq!(
                [
                    state.headgear_style,
                    state.shirt_style,
                    state.trousers_style,
                    state.footwear_style
                ],
                [1, 0, 0, 0]
            );
            let r = state.get_char_gen_result();
            assert_eq!(
                [
                    r.headgear_color,
                    r.shirt_color,
                    r.trousers_color,
                    r.footwear_color
                ],
                [6, 2, 3, 3]
            );
            assert_eq!(state.frozen, [true; 3]);
        }
        let fixed = (
            state.heritage_group,
            state.gender,
            state.template,
            state.start_area,
        );
        state.prepare_summary(&t);
        assert_eq!(
            (
                state.heritage_group,
                state.gender,
                state.template,
                state.start_area
            ),
            fixed
        );
        assert!(state.shirt_style >= 0);
        assert_eq!(state.frozen, [true; 3]);
        let result = state.get_char_gen_result();
        let next = state.rng.crt.clone().next_u16();
        state.prepare_summary(&t);
        assert_eq!(state.get_char_gen_result(), result);
        assert_eq!(
            state.rng.crt.next_u16(),
            next,
            "a complete summary consumes no draws"
        );
    }
}

#[test]
fn modern_entry_preserves_existing_world_rolls_and_final_template_presentation() {
    for old in [false, true] {
        let t = tables(old);
        let mut direct = CharGenState::default();
        direct.clothing = Rc::clone(&t.clothing);
        direct.randomize_character(&t.chargen, &t.skills, false);
        let mut policy = CharGenState::with_policy(CreationPolicy::Modern);
        policy.begin_creation(&t, CreationEntry::Normal, false);
        assert_eq!(policy.get_char_gen_result(), direct.get_char_gen_result());
        assert_eq!(policy.rng.crt.next_u16(), direct.rng.crt.next_u16());
        assert_eq!(policy.rng.ran2.next_f64(), direct.rng.ran2.next_f64());
        if !old {
            for h in t.chargen.heritage_groups.values() {
                for &sex in &h.sex_order {
                    for (i, template) in h.templates.iter().enumerate() {
                        let p = h.template_presentation(sex, i).unwrap();
                        assert_eq!(p.icon, template.icon);
                        assert!(p.description.is_none());
                    }
                }
            }
        }
    }
}

#[test]
fn classic_choices_keep_world_keys_and_templates_keep_unavailable_skills_inactive() {
    let mut t = tables(true);
    let mut h = t.chargen.heritage_groups.remove(&1).unwrap();
    let first = h.sexes.remove(&2).unwrap();
    let second = h.sexes.remove(&1).unwrap();
    h.sexes = BTreeMap::from([(9, first), (5, second)]);
    h.sex_order = vec![9, 5];
    h.template_presentations.clear();
    h.templates[1].normal_skills = vec![1, 2, 54];
    h.templates[1].primary_skills = vec![1, 2, 54];
    h.skills.push((2, -1, -1));
    t.chargen.heritage_groups = BTreeMap::from([(42, h)]);
    t.chargen.heritage_order = vec![42];
    t.skills.skills.remove(&1);
    let mut state = CharGenState::with_policy(CreationPolicy::Classic);
    state.begin_creation(&t, CreationEntry::Normal, false);
    assert_eq!(state.heritage_group, 42);
    assert!(matches!(state.gender, 9 | 5));
    assert!(!state.choose_heritage(&t, 1));
    assert!(!state.choose_gender(&t, 2));
    assert!(state.choose_gender(&t, 9));
    assert!(state.choose_template(&t, 1));
    for skill in [1, 2, 54] {
        assert_eq!(state.skill_levels[skill], SkillAdvancementClass::Inactive);
    }
    assert!(state.choose_template(&t, -1));
    assert_eq!(state.template, 0);
    assert!(!state.choose_template(&t, -2));
    let result = state.get_char_gen_result();
    assert_eq!((result.heritage_group, result.gender), (42, 9));
    assert_eq!(result.skill_advancement_classes.len(), 55);
}

#[test]
fn classic_page_random_attributes_preserve_locks_and_skills_use_the_world_range() {
    let t = tables(true);
    let mut state = CharGenState::with_policy(CreationPolicy::Classic);
    state.begin_creation(&t, CreationEntry::Normal, false);
    state.rng = CharGenRng::default();
    state.randomize_page(&t, CreationRandom::Attributes, false);
    assert_eq!(
        Attr::BALANCE_ORDER.map(|a| state.get(a)),
        [25, 80, 43, 25, 67, 90]
    );
    assert_eq!(state.rng.crt.clone().next_u16(), 24464);
    state.set_locked(Attr::Strength, true);
    let locked = state.get(Attr::Strength);
    state.randomize_page(&t, CreationRandom::Attributes, false);
    assert_eq!(state.get(Attr::Strength), locked);
    assert!(Attr::ALL.iter().map(|a| state.get(*a)).sum::<i32>() <= 330);
    state.rng = CharGenRng::default();
    state.randomize_page(&t, CreationRandom::Skills, false);
    assert_eq!(state.remaining_skill_credits, 0);
    assert_eq!(state.rng.crt.clone().next_u16(), 3902);
    assert_eq!(
        state
            .skill_levels
            .iter()
            .enumerate()
            .filter_map(|(i, s)| (*s == SkillAdvancementClass::Specialized).then_some(i))
            .collect::<Vec<_>>(),
        [19, 23, 33, 36]
    );
    assert_eq!(state.skill_levels.len(), 55);
    assert!(state.skill_levels[41..]
        .iter()
        .all(|s| *s == SkillAdvancementClass::Inactive));
}

#[test]
fn skill_changes_refund_prior_cost_and_refuse_unaffordable_or_unavailable_classes() {
    for old in [true, false] {
        let t = tables(old);
        let mut state = CharGenState::default();
        state.set_heritage_group(&t.chargen, &t.skills, 1);
        state.set_gender(&t.chargen, 1);
        state.reset_skill_levels(&t.chargen, &t.skills);
        let (id, trained, specialized) = t
            .skills
            .skills
            .keys()
            .find_map(|&id| {
                let (trained, specialized) =
                    CharGenState::skill_costs(&t.chargen, &t.skills, 1, id);
                (trained > 0 && specialized > trained && specialized <= 50).then_some((
                    id,
                    trained,
                    specialized,
                ))
            })
            .unwrap();
        state.total_skill_credits = trained;
        state.update_remaining_skill_credits(&t.chargen, &t.skills);
        state.set_skill_level(
            &t.chargen,
            &t.skills,
            id,
            SkillAdvancementClass::Specialized,
        );
        assert_eq!(state.skill_level(id), SkillAdvancementClass::Untrained);
        assert_eq!(state.remaining_skill_credits, trained);
        state.set_skill_level(&t.chargen, &t.skills, id, SkillAdvancementClass::Trained);
        assert_eq!(state.remaining_skill_credits, 0);
        state.set_skill_level(
            &t.chargen,
            &t.skills,
            id,
            SkillAdvancementClass::Specialized,
        );
        assert_eq!(state.skill_level(id), SkillAdvancementClass::Trained);
        assert_eq!(state.remaining_skill_credits, 0);
        state.total_skill_credits = specialized;
        state.update_remaining_skill_credits(&t.chargen, &t.skills);
        state.set_skill_level(
            &t.chargen,
            &t.skills,
            id,
            SkillAdvancementClass::Specialized,
        );
        assert_eq!(state.skill_level(id), SkillAdvancementClass::Specialized);
        assert_eq!(state.remaining_skill_credits, 0);
        state.set_skill_level(&t.chargen, &t.skills, id, SkillAdvancementClass::Untrained);
        assert_eq!(state.remaining_skill_credits, specialized);
        state.set_skill_level(&t.chargen, &t.skills, id, SkillAdvancementClass::Inactive);
        assert_eq!(state.skill_level(id), SkillAdvancementClass::Untrained);
    }
}

#[test]
fn classic_profession_random_applies_twice_and_explicit_custom_applies_row_zero() {
    let t = tables(true);
    let mut state = CharGenState::with_policy(CreationPolicy::Classic);
    state.begin_creation(&t, CreationEntry::Normal, false);
    state.rng = CharGenRng::default();
    state.randomize_page(&t, CreationRandom::Template, false);
    assert_eq!(state.template, 1);
    assert_eq!(
        state.rng.crt.next_u16(),
        26500,
        "selection then two singleton profile draws"
    );
    state.set_attribute(Attr::Strength, 100);
    state.rng = CharGenRng::default();
    assert!(state.choose_template(&t, -1));
    assert_eq!(state.template, 0);
    assert_ne!(
        state.get(Attr::Strength),
        100,
        "Custom replaces the edited build"
    );
    assert_eq!(
        state.rng.crt.next_u16(),
        18467,
        "explicit Custom applies its singleton profile"
    );
    state.set_attribute(Attr::Strength, 100);
    assert!(state.choose_template(&t, 0));
    assert_ne!(
        state.get(Attr::Strength),
        100,
        "Custom replaces the edited build"
    );
}

#[test]
fn color_choices_sample_the_decoded_palette_layout() {
    use pyreal_assets::material::{Palette, PaletteSet};
    use pyreal_chargen::palette::{PaletteLayout, PaletteSample};
    use std::collections::BTreeSet;
    assert_eq!(PaletteLayout::from_entry_count(0), None);
    assert_eq!(PaletteLayout::from_entry_count(257), None);
    for old in [true, false] {
        let store = if old {
            pyreal_dat::testing::open_classic_store_or_fail()
        } else {
            RetailDatStore::open_dir(&pyreal_dat::testing::dat_dir()).unwrap()
        };
        let t = tables(old);
        let sex = &t.chargen.heritage_groups[&1].sexes[&2];
        let sample = |id: DataId, part: PaletteSample| {
            let p = Palette::decode_payload(id, &store.read_portal(id).unwrap()).unwrap();
            let layout = PaletteLayout::from_entry_count(p.colors_argb.len()).unwrap();
            p.colors_argb[part.index(layout)]
        };
        let eye_colors: BTreeSet<_> = sex
            .eye_colors
            .iter()
            .map(|&id| sample(DataId(id), PaletteSample::Eyes))
            .collect();
        assert_eq!(eye_colors.len(), sex.eye_colors.len());
        let hair_colors: BTreeSet<_> = sex
            .hair_colors
            .iter()
            .map(|&id| {
                let set =
                    PaletteSet::decode_payload(DataId(id), &store.read_portal(DataId(id)).unwrap())
                        .unwrap();
                sample(set.palette_ids[0], PaletteSample::Hair)
            })
            .collect();
        assert!(
            hair_colors.len() >= 3,
            "hair choices must not collapse to one unrelated color"
        );
        let set = PaletteSet::decode_payload(
            sex.skin_palset,
            &store.read_portal(sex.skin_palset).unwrap(),
        )
        .unwrap();
        let shades: BTreeSet<_> = set
            .palette_ids
            .iter()
            .map(|&id| sample(id, PaletteSample::Skin))
            .collect();
        assert_eq!(shades.len(), set.palette_ids.len());
    }
}

#[test]
fn classic_random_heritage_on_the_final_world_is_aluvian_gharundim_or_sho() {
    let t = tables(false);
    assert!(
        t.heritage_keys().len() > 3,
        "the final world carries more heritages than the three"
    );
    let classic = pyreal_chargen::CLASSIC_RANDOM_HERITAGES;
    let mut seen = std::collections::BTreeSet::new();
    for seed in 1..=60u32 {
        for entry in [CreationEntry::Normal, CreationEntry::Quick] {
            let mut state = CharGenState::with_policy(CreationPolicy::Classic);
            state.rng = CharGenRng::new(i32::try_from(seed).unwrap(), seed);
            state.begin_creation(&t, entry, true);
            assert!(
                classic.contains(&state.heritage_group),
                "seed {seed} {entry:?} opened on heritage {}",
                state.heritage_group
            );
            seen.insert(state.heritage_group);
            for _ in 0..4 {
                state.randomize_page(&t, CreationRandom::Heritage, true);
                assert!(
                    classic.contains(&state.heritage_group),
                    "seed {seed} Random gave heritage {}",
                    state.heritage_group
                );
                seen.insert(state.heritage_group);
            }
            // An explicit pick of another heritage stays possible, and Random leaves it again.
            assert!(state.choose_heritage(&t, pyreal_chargen::HERITAGE_GEAR_KNIGHT));
            state.randomize_page(&t, CreationRandom::Heritage, true);
            assert!(classic.contains(&state.heritage_group));
        }
    }
    assert_eq!(
        seen.into_iter().collect::<Vec<_>>(),
        classic,
        "all three are reached"
    );
}

#[test]
fn modern_random_heritage_never_rolls_a_heritage_the_world_lacks() {
    let t = tables(true);
    assert_eq!(
        t.heritage_keys(),
        [1, 2, 3],
        "the earlier world has no Viamontian"
    );
    let mut seen = std::collections::BTreeSet::new();
    for seed in 1..=60 {
        let mut state = CharGenState::with_policy(CreationPolicy::Modern);
        state.rng = CharGenRng::new(seed, 1);
        state.begin_creation(&t, CreationEntry::Normal, true);
        assert!(
            t.chargen
                .heritage_groups
                .contains_key(&state.heritage_group),
            "seed {seed} opened on heritage {}",
            state.heritage_group
        );
        seen.insert(state.heritage_group);
        for _ in 0..4 {
            state.randomize_page(&t, CreationRandom::Heritage, true);
            assert!(
                t.chargen
                    .heritage_groups
                    .contains_key(&state.heritage_group),
                "seed {seed} Random gave heritage {}",
                state.heritage_group
            );
            seen.insert(state.heritage_group);
        }
    }
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), [1, 2, 3]);
}

#[test]
fn every_preset_fits_the_worlds_tables_for_each_heritage_it_names() {
    use pyreal_chargen::presets::{presets, Fit};
    use pyreal_primitives::EraId;
    for (era, old) in [
        (EraId::Launch, true),
        (EraId::Infiltration, true),
        (EraId::Eor, false),
    ] {
        let t = tables(old);
        let heritages: Vec<u32> = t
            .heritage_keys()
            .iter()
            .copied()
            .filter(|h| !matches!(*h, 12 | 13))
            .collect();
        for p in presets(era) {
            let named = if p.heritages().is_empty() {
                heritages.clone()
            } else {
                p.heritages()
            };
            for h in &named {
                let fit = p.fit(&t, *h, era.features());
                assert!(
                    matches!(fit, Fit::Usable { .. }),
                    "{era:?} {} for heritage {h}: {fit:?}",
                    p.id
                );
            }
            for h in heritages.iter().filter(|h| !named.contains(h)) {
                assert_eq!(
                    p.fit(&t, *h, era.features()),
                    Fit::NeedsHeritage(named.clone()),
                    "{era:?} {} for heritage {h}",
                    p.id
                );
            }
        }
    }
    // The credits the plan's tables give, spot-checked.
    let fit = |era: EraId, old: bool, id: &str, h: u32| {
        let t = tables(old);
        let p = presets(era).iter().find(|p| p.id == id).unwrap();
        p.fit(&t, h, era.features())
    };
    let usable = |used, left| Fit::Usable { used, left };
    assert_eq!(fit(EraId::Infiltration, true, "battle", 1), usable(48, 2));
    assert_eq!(fit(EraId::Infiltration, true, "archer", 1), usable(50, 0));
    assert_eq!(fit(EraId::Infiltration, true, "archer", 3), usable(50, 0));
    assert_eq!(fit(EraId::Infiltration, true, "hfight", 2), usable(46, 4));
    assert_eq!(fit(EraId::Launch, true, "fourmana", 2), usable(50, 0));
    assert_eq!(fit(EraId::Eor, false, "battle", 5), usable(48, 4));
    assert_eq!(fit(EraId::Eor, false, "pvm", 9), usable(46, 6));
    assert_eq!(fit(EraId::Eor, false, "cov", 1), usable(52, 0));
    // A Launch world loses what is left, so a preset that banks is hidden there.
    assert_eq!(
        presets(EraId::Infiltration)[2].fit(&tables(true), 1, EraId::Launch.features()),
        Fit::Unusable
    );
}

#[test]
fn a_preset_applies_as_a_custom_character_follows_the_heritage_and_knows_when_it_was_changed() {
    use pyreal_chargen::presets::{presets, INFILTRATION};
    use pyreal_primitives::EraId;
    use SkillAdvancementClass::{Specialized, Trained, Untrained};
    let t = tables(true);
    let mut s = CharGenState::with_policy(CreationPolicy::Modern);
    s.begin_creation(&t, CreationEntry::Normal, false);
    assert!(s.choose_heritage(&t, 3));
    assert!(s.choose_template(&t, 2));
    let hf = presets(EraId::Infiltration)
        .iter()
        .find(|p| p.id == "hfight")
        .unwrap();
    assert!(s.apply_preset(&t, hf));
    assert_eq!(s.template, -1);
    assert_eq!(s.preset.map(|p| p.id), Some("hfight"));
    assert_eq!(
        Attr::BALANCE_ORDER.map(|a| s.get(a)),
        [100, 10, 100, 10, 100, 10]
    );
    assert_eq!(s.remaining_atrb_credits, 0);
    assert_eq!(s.skill_levels[13], Specialized);
    assert_eq!(s.skill_levels[6], Specialized);
    assert_eq!(s.skill_levels[21], Trained);
    assert_eq!(s.remaining_skill_credits, 2);
    assert!(!s.preset_changed());

    // Gharu'ndim take the same preset's staff variant.
    assert!(s.choose_heritage(&t, 2));
    assert_eq!(s.preset.map(|p| p.id), Some("hfight"));
    assert_eq!(s.skill_levels[10], Specialized);
    assert_eq!(s.skill_levels[13], Untrained);
    assert_eq!(s.remaining_skill_credits, 4);
    assert!(!s.preset_changed());

    // A change shows, and an undone change does not.
    s.set_skill_level(&t.chargen, &t.skills, 33, Untrained);
    assert!(s.preset_changed());
    s.set_skill_level(&t.chargen, &t.skills, 33, Trained);
    assert!(!s.preset_changed());
    s.set_attribute(Attr::Self_, 20);
    assert!(s.preset_changed());

    // Aluvians have no variant: Custom, keeping what was chosen.
    assert!(s.choose_heritage(&t, 1));
    assert!(s.preset.is_none());
    assert!(!s.preset_changed());
    assert_eq!(s.template, -1);
    assert_eq!(Attr::BALANCE_ORDER.map(|a| s.get(a))[5], 20);
    assert!(s.remaining_skill_credits >= 0);
    assert!(!s.apply_preset(&t, hf));

    // Choosing a world template forgets the preset.
    assert!(s.apply_preset(&t, &INFILTRATION[0]));
    assert!(s.choose_template(&t, 1));
    assert!(s.preset.is_none());
    let result = s.get_char_gen_result();
    assert_eq!(result.skill_advancement_classes.len(), 55);
}

/// An Infiltration world on the end-of-retail files (Empyrean's): every original heritage and
/// template goes to its server as a template, with only the era's skills and none the table lacks.
#[test]
fn a_2005_world_on_the_later_files_is_sent_only_what_its_server_takes() {
    use pyreal_primitives::EraId;
    let t = tables(false);
    let era = EraId::Infiltration
        .creation_skills()
        .expect("the era's skills");
    for heritage in [1, 2, 3] {
        let templates = t.chargen.heritage_groups[&heritage].templates.len();
        for template in -1..i32::try_from(templates).unwrap() {
            let mut s = CharGenState::with_policy(CreationPolicy::Modern);
            s.begin_creation(&t, CreationEntry::Normal, false);
            s.set_era_skills(&t, Some(era));
            assert!(s.choose_heritage(&t, heritage));
            if template >= 0 {
                assert!(s.choose_template(&t, template));
            } else {
                s.template = -1;
            }
            let r = s.result_for_server(&t.chargen);
            assert!(r.template_num >= 0, "{heritage}/{template}");
            assert!(s.remaining_skill_credits >= 0, "{heritage}/{template}");
            for (id, class) in (0u32..).zip(&r.skill_advancement_classes) {
                if *class == SkillAdvancementClass::Inactive as i32 {
                    continue;
                }
                assert!(
                    t.skills.skills.contains_key(&id),
                    "{heritage}/{template}: {id}"
                );
                if *class > SkillAdvancementClass::Untrained as i32 {
                    assert!(era.contains(&id), "{heritage}/{template}: {id}");
                }
            }
        }
    }
}
