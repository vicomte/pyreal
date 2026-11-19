//! The read-only view of the game model the interface draws from.

use super::*;

pub trait GameView: std::fmt::Debug {
    /// What the world's era means for what can be shown ([`EraView`]); `None` from a view that
    /// has no world.
    fn era(&self) -> Option<&EraView> {
        None
    }

    /// Resolved world systems; a view without a world uses the default profile.
    fn era_features(&self) -> pyreal_primitives::EraFeatures {
        self.era().map_or(
            pyreal_primitives::EraFeatures::END_OF_RETAIL,
            EraView::features,
        )
    }
    fn era_ui(&self) -> crate::era::EraUiFacts {
        self.era()
            .map_or_else(crate::era::EraUiFacts::default, |e| {
                crate::era::EraUiFacts::for_world(e.era, e.features())
            })
    }
    fn aetheria_slots(&self) -> u8 {
        let unlocks = self
            .player()
            .and_then(|p| self.int_stat(p, 0x142))
            .unwrap_or(0);
        crate::era::aetheria_slots(self.era_features(), unlocks)
    }

    /// The player object, once `LOGIN_COMPLETE` (global message `0x0B`) has fired.
    fn player(&self) -> Option<ObjectId> {
        None
    }
    fn name(&self, _id: ObjectId) -> Option<&str> {
        None
    }
    fn icon(&self, _id: ObjectId) -> Option<DataId> {
        None
    }
    /// The requested integer quality.
    fn int_stat(&self, _id: ObjectId, _prop: u32) -> Option<i32> {
        None
    }
    /// The queried secondary attribute as `(current, max)`.
    fn vital(&self, _id: ObjectId, _which: Vital) -> Option<(u32, u32)> {
        None
    }
    fn container_contents(&self, _id: ObjectId) -> &[ObjectId] {
        &[]
    }
    /// Whether a Loot All run is moving the ground container's items.
    fn loot_all_running(&self) -> bool {
        false
    }
    /// The contained-containers list — the **side packs**, which are a
    /// separate ordered list from [`Self::container_contents`] and are what a
    /// `UI_ItemList_IsContainer` list shows.
    fn contained_containers(&self, _id: ObjectId) -> &[ObjectId] {
        &[]
    }
    /// The inventory-placement list — `(iid, loc)` per wielded or worn item.
    ///
    /// The paper doll's inventory rebuild walks exactly this list and places each `(iid, loc)`
    /// into its doll location, which is the only source the equipment doll has.
    fn equipment(&self, _id: ObjectId) -> &[(ObjectId, u32)] {
        &[]
    }
    /// The loose-item count used by the item list's container-size update
    /// writes into `UI_ItemList_FixedListSize` for a non-container list. `None` when the object is
    /// unknown; a negative value is the client's "unbounded" and selects the empty-slot update.
    fn items_capacity(&self, _id: ObjectId) -> Option<i32> {
        None
    }
    /// The selected object's id.
    fn selection(&self) -> Option<ObjectId> {
        None
    }
    /// Radar objects already converted to player space by the smart box.
    fn radar_objects(&self) -> &[RadarEntry] {
        &[]
    }
    /// The burden indicator's only input.
    fn load(&self) -> Option<f32> {
        None
    }
    /// The helpful and harmful enchantment collections.
    fn enchantment_counts(&self) -> (u32, u32) {
        (0, 0)
    }
    /// Active enchantments joined to the `SpellTable`, in no particular order; the
    /// effects panel sorts them by name.
    ///
    /// The default is empty, matching the absent-player-description guard before
    /// `0x0013`: rebuilding leaves the list cleared rather than drawing an empty row.
    fn active_effects(&self) -> Vec<EffectEntry> {
        Vec::new()
    }
    /// The vitae multiplier, with `< 1.0` meaning a penalty is active.
    fn vitae(&self) -> Option<f32> {
        None
    }
    /// The player's charge on a spell, 0.5 to 1.0, on a world with charges. `None` elsewhere.
    fn spell_charge(&self, _spell: u32) -> Option<f32> {
        None
    }
    /// The three vitae-display inputs, or `None` when no player description is
    /// available; that case writes nothing.
    fn vitae_display(&self) -> Option<VitaeDisplay> {
        None
    }
    /// The character sheet's inputs, or `None` when no player description is available;
    /// that case writes nothing.
    fn character_info(&self) -> Option<CharacterInfo> {
        None
    }
    /// The frame clock needed by panels that own a timer.
    ///
    /// Carried on the view because the retired retail UI's `panels::linkstatus::LinkStatusPanel`
    /// tracks its last ping request, next update and round trip, as the original panel
    /// does. The default `0.0` freezes those timers, which is what an unclocked host
    /// should get.
    fn now(&self) -> f64 {
        0.0
    }
    /// How many `0x01EA Character_ReturnPing` have arrived.
    ///
    /// A **count**, not a timestamp: the client computes
    /// the current time less the last ping request's time at notice-delivery time, and this build
    /// has no notice bus, so the panel does the subtraction on the frame it first sees the count
    /// move. The difference is at most one frame of the round trip and is stated rather than
    /// hidden.
    fn ping_returns(&self) -> u64 {
        0
    }
    /// The link-status holder's packet-loss figure.
    ///
    /// A **ratio**, not a percentage, in spite of the accessor's name and in spite of the shipped
    /// string putting a `%` after it: the average-packet-loss accessor is
    /// `2 * (NAKed + retransmitted) / (received + sent)` and nothing multiplies it by a hundred.
    ///
    /// The default is `1.0` because that is the packet-loss field's initial value
    /// (the client's constructor writes `1.0f` into that field), i.e. a client that
    /// has heard nothing from a server reports total loss. It is **not** an `Option`: in retail
    /// the link-status panel's update sets its packet-loss line's float variable unconditionally —
    /// a `????` belongs to the ping line alone.
    fn packet_loss_percent(&self) -> f32 {
        crate::linkstatus::INITIAL_PACKET_LOSS
    }
    /// The portal-storm level, as the portal-storm-level notice carries it.
    ///
    /// **A `f32`, not an integer.**
    /// The portal-storm-level notice takes an `M` (a `float`) and
    /// the storm indicator's update compares it with `0.0` as a float.
    /// The level on the wire
    /// is `0x02C9`/`0x02CA`'s `extent`, which ACE sends as a fraction — so truncating it to an
    /// integer would turn every storm below 1.0 into "no storm".
    fn portal_storm_level(&self) -> f32 {
        0.0
    }
    /// A character option bit read by the Character Options page.
    ///
    /// `pyreal_client_runtime::hud::HudView` implements it. With the trait's `false` as the only answer,
    /// all 50 rows of the Character Options page would open unticked whatever the server had
    /// sent, which is why `crate::options::character::CharacterSettingsPage::values_seen`
    /// exists.
    ///
    /// The default is still `false` rather than an `Option`, because that is what the *page* can
    /// use; a host that wants to tell "off" from "not asked" apart asks
    /// `pyreal_client_runtime::hud::character_option`, which answers `Option<bool>`.
    fn player_option(&self, _o: PlayerOption) -> bool {
        false
    }

    /// Read a floating-point gameplay option by property id — the read
    /// half of the client's named-property arm.
    ///
    /// The only two callers are the Chat Options page's opacity sliders, `0x10000080` and
    /// `0x10000081`, and both are **top-level** properties of the gameplay options — no window
    /// index, because the general section is one pair of sliders for all five chat windows.
    ///
    /// `None` is a module that carries no value, which is not a module that carries `0.0`: the
    /// client's fallback is the option's default, which is `0.5` and `1.0` out of
    /// property collection `0x78000001`, and a zero would make the chat window invisible.
    fn gameplay_option_float(&self, _property: u32) -> Option<f32> {
        None
    }

    /// Read property `0x1000007F` for one chat window — that window's
    /// 64-bit text-type filter, the read half of the 64-bit check box's
    /// value getter.
    ///
    /// The chat-option structure lookup indexes the gameplay options' `0x1000008C` array by
    /// the window id less 1, so this is per window and a window with no row in that array answers
    /// `None` — which the page turns into the window's own default mask, exactly as
    /// the value getter's default-preloaded result does.
    fn chat_window_filter(&self, _window: u32) -> Option<u64> {
        None
    }

    /// The four facts the client's not-a-stack arm branches
    /// on before it sends a health query or an item-mana query.
    ///
    /// `None` is *"this host does not answer the question"* and is not the same as a selection
    /// that asks for neither query: the first must not read as the second, or a build with no
    /// producer looks exactly like a build whose selection genuinely wanted nothing. See
    /// the retired retail UI's `screens::gameplay::ToolbarSelection::query`.
    fn selection_query_facts(&self, _id: ObjectId) -> Option<SelectionQueryFacts> {
        None
    }

    /// The toolbar's selected-object meters: `(health, mana)`, each 0.0–1.0 or `None`
    /// for no answer yet.
    ///
    /// These are the *only* place the retail client keeps either fraction:
    /// the object-health notice and the item-mana notice write
    /// `METER_LEVEL` straight from the `0x01C0` / `0x0264` reply and nothing stores it per object.
    /// `pyreal_client_model::combat::SelectedMeters` holds them, and this is their reader.
    fn selected_meters(&self) -> (Option<f32>, Option<f32>) {
        (None, None)
    }
    /// the AC N/E coordinate pair.
    fn player_coords(&self) -> Option<(f32, f32)> {
        None
    }
    /// The game-time string off the client's current game time —
    /// `(date, time-of-day name)`, already formatted, for the map panel's first
    /// block.
    ///
    /// `pyreal_scene::sky::GameClock` drives the sky off that object, and this seam is how the
    /// map panel asks it: the element is one retail updates unconditionally every five seconds.
    ///
    /// `None` is retail's no-current-game-time arm, which is **not** a blank
    /// element: see `crate::mapradar::map::date_time_text`.
    ///
    /// This is *not* gated on being outdoors. The is-outside call sits below this
    /// block and guards only [`Self::player_coords`]' two consumers.
    fn game_date_time(&self) -> Option<(String, String)> {
        None
    }
    /// The is-player-outside test, which is
    /// also what picks the radar's 75-vs-25 range and the chat sweep's radius.
    ///
    /// `pyreal_client_runtime::hud::HudView` implements it off the player's own `objcell_id`. With the
    /// trait's `true` as the only answer, the radar would draw at the outdoor 75 inside every
    /// building and every dungeon.
    ///
    /// The default stays `true` rather than becoming an `Option`: a host that cannot answer is
    /// outdoors as far as every screen is concerned, and the two arms are the only two the client
    /// has. What separates "outdoors" from "not asked" is whether the host implements it, which
    /// is a compile-time question and not a runtime one.
    fn player_outside(&self) -> bool {
        true
    }
    /// Whether the player's character has the administrator flag, the one the server's Admin
    /// commands check.
    fn player_is_admin(&self) -> bool {
        false
    }
    /// The player's heading, in degrees.
    fn player_heading(&self) -> f32 {
        0.0
    }

    /// The text properties at the top of the player module's gameplay-options collection, which
    /// the server keeps for the character and returns at login: `None` before the module has
    /// arrived, and an empty list when it holds none.
    fn player_module_strings(&self) -> Option<Vec<(u32, String)>> {
        None
    }

    /// The object id held by quickbar slot `slot`.
    ///
    /// A player-description update walks all eighteen shortcut entries and adds every
    /// nonempty entry to its corresponding slot. These are `PlayerModule` state, carried
    /// by `0x0013 Login_PlayerDescription` and saved by the 480-second flush, rather
    /// than object-table state. A shortcut may name an object the client has never
    /// seen; that produces a delayed slot rather than an empty one.
    fn shortcut(&self, _slot: u32) -> Option<ObjectId> {
        None
    }

    /// The `SkillTable` joined to the player's quality state.
    ///
    /// One entry per `SkillTable` key, in the table's own key order; the panel does the grouping
    /// and the alphabetical sort rather than requiring the host to provide display order.
    fn skills(&self) -> &[SkillEntry] {
        &[]
    }

    /// The player's known spell IDs joined to the spell table.
    fn spellbook(&self) -> &[SpellEntry] {
        &[]
    }

    /// The most recent newly learned spell, as a receipt serial and spell id.
    fn last_learned_spell(&self) -> Option<(u64, u32)> {
        None
    }

    /// The most recent confirmed successful formula test.
    fn research_success(&self) -> Option<crate::research::ResearchSuccess> {
        None
    }

    /// Look up `spell_id` in the spell table — **one row, whether
    /// or not the player knows the spell.**
    ///
    /// This is deliberately not `spellbook().iter().find(...)`, for the reason
    /// [`Self::is_spell_known`] gives in the other direction: `spellbook()` is the player's
    /// spell book joined to the table, and the two questions have different answers exactly
    /// where it matters here. The spellcasting panel's cast-button tooltip and
    /// its endowment icon both ask the spell table about
    /// **the endowment spell** — the spell on the wand in the player's hand — and a wand's spell
    /// is ordinarily not in the player's book at all. Answering from the book would leave the
    /// caption and the endowment icon blank for every caster in the game.
    ///
    /// The row is reduced to [`SpellEntry`] because that is already what a spell-bar row carries
    /// and what `crate::items::widget::spell_recipe` needs; `id` is echoed back so a caller can
    /// pass the entry on unchanged.
    ///
    /// The default is the join, which is all a host without the table can say.
    fn spell(&self, spell_id: u32) -> Option<SpellEntry> {
        self.spellbook().iter().find(|s| s.id == spell_id).cloned()
    }

    /// The spell's category in the spell table: the spells of one category are one family (the
    /// levels of a buff and its higher-level namesakes), of which a character holds the strongest.
    /// `None` where the host has no spell table, or the spell has no category (0).
    fn spell_category(&self, _spell_id: u32) -> Option<u32> {
        None
    }

    /// The spell's difficulty (its power) and base mana cost in the spell table, or `None` where
    /// the host has no spell table.
    fn spell_cost(&self, _spell_id: u32) -> Option<(i32, i32)> {
        None
    }

    /// The spell's duration in seconds in the spell table, or `None` where the host has no spell
    /// table or the spell does not run out.
    fn spell_duration(&self, _spell_id: u32) -> Option<f64> {
        None
    }

    /// Whether the spell table has the spell: [`GameView::spell`] without building the row.
    fn has_spell(&self, spell_id: u32) -> bool {
        self.spell(spell_id).is_some()
    }

    /// The spell's description in the spell table, or `None` where the host has none.
    fn spell_description(&self, _spell_id: u32) -> Option<String> {
        None
    }

    /// Is `spell` a key of the player's spell book?
    ///
    /// This is *not* [`Self::spellbook`]`.contains`, and the difference is the whole reason it
    /// exists. `spellbook()` is the book **joined to the `SpellTable`**, so a spell the character
    /// knows whose id has no table row is absent from it. The runtime
    /// prunes a favourite that fails the known-spell test **and tells the shard**
    /// (`0x01E4`), so answering that question with the join would put an unrequested edit on the
    /// wire for a spell whose only problem is missing metadata.
    ///
    /// The default is the join, which is all a host without a `PlayerDesc` can say.
    fn is_spell_known(&self, spell_id: u32) -> bool {
        self.spellbook().iter().any(|s| s.id == spell_id)
    }

    /// The spell id joined to the component table — everything
    /// the spell-examine pane reads for one spell.
    ///
    /// `None` is the client's no-magic-system / unknown-spell arm: the pane is not
    /// filled and the window is not shown.
    fn spell_examine(&self, _spell_id: u32) -> Option<SpellExamineView> {
        None
    }

    /// The component tracker's is-owned test for a formula slot's SCID — whether the
    /// player holds any of that component.
    ///
    /// The default is `false`, which is deliberately the *marked* answer: a host with no
    /// component tracker takes the client's show-the-mark arm, so a view
    /// that cannot answer shows the "you do not have this" mark rather than hiding it.
    fn component_is_owned(&self, _scid: u32) -> bool {
        false
    }

    /// The non-raw attribute query for `id` — the number
    /// the attribute row writes into its value text, for one of the six
    /// primary attributes (1 Strength ... 6 Self). `None` renders as `"???"`.
    ///
    /// Not `raw`: the displayed value is the enchanted one, which is why a buffed character's
    /// attributes page reads higher than their creation profile.
    fn attribute(&self, _id: u32) -> Option<i32> {
        None
    }

    /// The skill footer's inputs for one skill; see [`SkillAdvancement`].
    fn skill_advancement(&self, _id: u32) -> Option<SkillAdvancement> {
        None
    }

    /// The attribute footer's inputs for one of nine rows; see
    /// [`AttributeAdvancement`].
    ///
    /// `secondary` distinguishes the three vital rows from the six primary attributes.
    /// The original row-kind query returns 8 for a primary and a different value for
    /// a vital. The flag cannot be derived from the stat id because the id spaces
    /// overlap: stat 2 is Endurance as a primary and Health as a secondary.
    fn attribute_advancement(&self, _id: u32, _secondary: bool) -> Option<AttributeAdvancement> {
        None
    }

    /// The local player character's name for the stat-panel header.
    ///
    /// Separate from [`Self::name`] because it specifically selects the local player.
    /// The host can answer from the received player description before the player's
    /// world object exists; the vitals panel uses the same early-availability seam.
    fn character_name(&self) -> Option<&str> {
        None
    }

    /// The journal's pages and state.
    fn journal(&self) -> crate::journal::JournalView {
        crate::journal::JournalView::default()
    }

    /// The client's inputs.
    fn experience_header(&self) -> Option<crate::statmgmt::XpHeader> {
        None
    }

    /// Resolve the display text from gender, heritage, and creature type for the
    /// player -- the heritage text's first half.
    ///
    /// The join is the host's for the same reason [`SkillEntry`]'s name is: the gender and
    /// heritage display-name lookups (enum `0x10000001` for gender, `0x10000002` for
    /// heritage group) against dat objects this crate must not know about. `None` is
    /// the gender/heritage display lookup returning 0, which leaves the field empty.
    fn gender_heritage_display(&self) -> Option<String> {
        None
    }

    /// The title-table lookup for the title id -- the title
    /// the stat panel's character-info update appends to the heritage text after a single space, and
    /// only when the lookup succeeds.
    fn display_title(&self) -> Option<String> {
        None
    }

    /// The titles panel's whole refresh input; see [`CharacterTitles`].
    ///
    /// Separate from [`Self::display_title`] because the header needs one resolved
    /// string while the Titles tab needs every id with its string. The tab also needs
    /// the current displayed title id to decide whether to offer the Display button.
    fn character_titles(&self) -> CharacterTitles {
        CharacterTitles::default()
    }

    /// The PK-status line -- see [`PkStatus`].
    fn pk_status(&self) -> PkStatus {
        PkStatus::default()
    }

    /// Integer qualities 6 and 7 -- `AvailableLuminance` and
    /// `MaximumLuminance`, the luminance pair's only inputs.
    ///
    /// The client reads both unconditionally and then decides whether to draw them; the level
    /// gate is the experience update's and lives in the panel, not here.
    fn luminance(&self) -> (i64, i64) {
        (0, 0)
    }

    /// Integer quality `0x18` — **available skill credits**, the number the skill panel's
    /// default footer and its untrained-selection footer both
    /// put on the footer's second and first lines respectively.
    fn skill_credits(&self) -> i64 {
        0
    }

    /// 64-bit integer quality 2 — **unassigned experience**, the other footer number and the
    /// one the raise buttons are affordable against.
    fn available_experience(&self) -> i64 {
        0
    }

    /// The fourteen filter bits
    /// the spellbook's filter test tests, **not** the button states.
    /// The player-module decoder defaults it to `0x3FFF` when section `0x0020` is absent, which is
    /// every school and every level.
    fn spell_filters(&self) -> u32 {
        crate::spellbook::DEFAULT_SPELL_FILTERS
    }

    /// The eight spell bars, which are the contents of the spell-casting submenu's item list.
    ///
    /// The spell bar's player-module refresh fills each tab's item list from this
    /// list, and the quick-cast notice indexes the *list*, so this is the
    /// order a quick-cast key resolves against. A tab outside `0..8` is empty.
    fn spell_tab(&self, _tab: usize) -> &[u32] {
        &[]
    }

    /// The UI item's decoration fields for one object — everything
    /// the UI item's per-frame update reads off the weenie **after** the icon, and
    /// nothing else.
    ///
    /// One method instead of separate field accessors because all four decoration
    /// updates run together for every list slot. A single `PublicWeenieDesc` lookup
    /// avoids four hash probes.
    fn slot_decoration(&self, _id: ObjectId) -> Option<SlotDecoration> {
        None
    }

    /// Where the object can be worn or wielded: its valid-locations mask from its description,
    /// `0` for a thing that cannot be (or an object unknown).
    fn equip_locations(&self, _id: ObjectId) -> u32 {
        0
    }

    /// Whether the object is attuned (it cannot be given away or dropped), as far as its
    /// appraisal is known; `false` before it has been appraised.
    fn item_attuned(&self, _id: ObjectId) -> bool {
        false
    }

    /// The object's `AppraisalProfile` projected for the examination panel.
    ///
    /// `None` means no `0x00C9 Item_SetAppraiseInfo` has arrived for that object — which is what
    /// makes the pull in the retired retail UI's `panels::examination::ExaminationPanel::update` equivalent to the
    /// client's push: the profile cannot be answered before the reply lands.
    ///
    /// It carries only the six fields the panel reads, for the reason [`SlotDecoration`] does the
    /// same for a slot: the profile is a hundred-odd properties, this crate cannot see
    /// `pyreal_protocol`, and inventing a wider seam would only hide which of them anything uses.
    fn appraisal(&self, _id: ObjectId) -> Option<AppraisalView> {
        None
    }

    /// Live `PublicWeenieDesc` facts for the item-examine inscription mouse handler:
    /// the inscribable bit and current equipment location. `None` means the object
    /// lookup missed. Hook-appraisal facts are deliberately not substituted; this
    /// handler reads the live object itself.
    fn inscription_mouse_facts(&self, _id: ObjectId) -> Option<(bool, u32)> {
        None
    }

    /// Send the examine-object notice for `id`, whose one listener is the examination panel.
    ///
    /// `(id, serial)`: the object the player last asked about, and how many times anything has
    /// asked. **The serial is the whole of why this is a pair rather than an `Option<ObjectId>`** —
    /// examining the *same* object twice must re-arm the awaited appraisal id and clear the
    /// current appraisal id, which is what re-opens a panel the player closed, and an
    /// id-only pull cannot see that. It is the counterpart of [`AppraisalView::delivery`] in the
    /// other direction and is a seam artefact of the pull model, declared as that one is.
    ///
    /// Three of the four routes that examine something live in `pyreal_client_runtime::interaction`, which
    /// has no screen to call: the examine **cursor**'s pick, the examine cursor's second click, and
    /// the `0x2B SELECTION_EXAMINE` key. The fourth, the toolbar's identify button, reaches the
    /// panel on the screen directly as well — the examine notice handler is idempotent for the same
    /// id, so arriving twice is harmless.
    fn examine_request(&self) -> Option<(ObjectId, u64)> {
        None
    }

    /// The open book's id, page list and inscription.
    ///
    /// `None` is `bookID == 0`: no `0x00B4 Writing_BookOpen` has arrived, or the last one was
    /// closed. The panel opens on a change of [`BookView::opening`] and on nothing else, which is
    /// the same pull-instead-of-push seam [`Self::appraisal`] carries.
    fn book_session(&self) -> crate::book::BookSessionView {
        crate::book::BookSessionView::default()
    }
    fn open_book(&self) -> Option<BookView> {
        None
    }

    /// The last `0x0075 Character_StartBarber` notice, projected with the local player's two
    /// appearance-table selectors.
    ///
    /// `None` means no barber modal is active. The generation is deliberately separate from the
    /// payload because retail opens a fresh modal for every notice, even when the values match.
    fn barber(&self) -> Option<BarberView> {
        None
    }

    /// The allegiance hierarchy, already walked.
    ///
    /// The default is an empty roster, which is what a character in no allegiance has — and what
    /// all twelve `0x0020 Allegiance_AllegianceUpdate` in the capture corpus produce, since every
    /// one of them carries `total_members = 0` and no member records.
    fn oath_xp_cost(&self) -> Option<u32> {
        None
    }
    fn allegiance_roster(&self) -> AllegianceRoster {
        AllegianceRoster::default()
    }

    /// The current fellowship, when one exists.
    ///
    /// The default is `None` — *no fellowship* — which is what every host that has never received
    /// a `0x02BE Fellowship_FullUpdate` answers, and it is a **different** state from a
    /// fellowship with one member: the panel branches on exactly this
    /// pointer to choose which of its two frames is on screen.
    fn fellowship(&self) -> Option<FellowshipView> {
        None
    }

    /// The AI Players in the player's company and their modes; empty from a world without
    /// Dereth's `ai_fellows` extension.
    fn ai_fellows(&self) -> Vec<AiFellowView> {
        Vec::new()
    }

    /// The friends list in model order.
    ///
    /// The default is **empty**, which is the same answer a host that has received a `0x0021`
    /// carrying no records gives -- and that is not a gap: all five recorded `0x0021` are
    /// exactly that, an empty list with type `Full`, so the empty case is the one the recorded
    /// corpus witnesses and the panel has to be able to say it drew it.
    /// the retired retail UI's `panels::friends::FriendsPanel::rebuilds` is what separates the two readings.
    ///
    /// The order is **not** the display order: the panel puts the
    /// online friends first and sorts each block by name, and that walk is the panel's.
    fn friends(&self) -> Vec<FriendEntry> {
        Vec::new()
    }

    /// The communication system's squelch iteration — the squelch DB's **character** hash,
    /// walked, with the
    /// empty entries and the account hash left out.
    ///
    /// It is the *communication system's* walk and not the panel's, which is why it crosses this
    /// seam already done rather than as a `SquelchDb`: the walk reaches into the communication
    /// system's own instance and asks each entry whether it is zoned and what name it carries,
    /// none of which this crate may know about.
    ///
    /// The default is **empty**, which is the same answer a host whose shard sent a `0x01F4` with
    /// an empty DB gives -- and that is the ordinary case, not a gap: every login clears the DB
    /// (the client clears it on login) and no recorded capture carries a populated one.
    /// the retired retail UI's `panels::squelch::SquelchPanel::rebuilds` is what separates the two readings.
    ///
    /// The order is **not** the display order: the panel's sorted insert sorts the
    /// rows by name as they are inserted, and that walk is the panel's.
    fn squelch_list(&self) -> Vec<SquelchEntry> {
        Vec::new()
    }

    /// Tracked contracts in tracker-table order, ascending by contract id.
    ///
    /// The default is empty, which is what every host that has received no `0x0314`/`0x0315`
    /// answers — and that is the overwhelmingly common case: neither opcode appears anywhere in
    /// the recorded corpus. So
    /// the retired retail UI's `panels::contracts::ContractsPanel::rebuilds` is what separates "a character with
    /// no contracts" from "the panel never ran".
    ///
    /// This is not display order: the panel re-sorts by name or status on every rebuild.
    fn contracts(&self) -> Vec<ContractEntry> {
        Vec::new()
    }

    /// How many `0x0226 House_HouseStatus` notices this session has received.
    ///
    /// The house panel's failed-transaction notice is a bare jump to its
    /// update, and the update redraws the pane unconditionally — it reads nothing out of
    /// the notice, whose one `u32` all three registered receivers ignore. So what crosses this
    /// seam is not the message's *content* (there is none the client keeps) but the fact that one
    /// **arrived**: the retired retail UI's `panels::house::HousePanel` redraws whenever this number moves, which
    /// is retail's "redraw on every notice" expressed in a pull.
    ///
    /// The default is `0`, which is what every host that has received no `0x0226` answers — and
    /// that is the case a build with no shard is in, where the pane falls back to
    /// the retired retail UI's `panels::house::HousePanel::stand_in_for_the_login_query`.
    fn house_status_notices(&self) -> u64 {
        0
    }

    /// House data shaped for the six sections that read it.
    ///
    /// `None` is retail's null pointer, which is both "no house" and "the last thing that arrived
    /// was a `0x0226`" — the status handler deletes the object.
    fn house_data(&self) -> Option<HouseDataView> {
        None
    }

    /// How many `0x0225 House_HouseData` have arrived.
    ///
    /// The house-data redraw edge, alongside [`Self::house_status_notices`]. The
    /// house pane redraws on every notice, so it redraws whenever this count changes.
    fn house_data_notices(&self) -> u64 {
        0
    }

    /// The client's two host-side inputs.
    fn house_purchase(&self) -> HousePurchaseView {
        HousePurchaseView::default()
    }

    /// What the CRT's `localtime` would add to a Unix instant before any of this crate's date
    /// formatters sees it, in seconds.
    ///
    /// Six functions in the retail image format a date and **all six** run their instant through
    /// `localtime` first (the house panel's purchase-time line, the character sheet's
    /// birth/age/deaths section, the account-banned handler, the chat scroll's own timestamp,
    /// the load-file variable substitution and the house system's time conversion). MSVCR70's
    /// `localtime` reads `TZ` if it is set and
    /// otherwise calls `GetTimeZoneInformation`, so the number is the **operating system's**,
    /// daylight rule included, and nothing in the client's own configuration takes part.
    ///
    /// It is an accessor and not a constant because the answer depends on the instant: a house
    /// bought in January and one bought in July are an hour apart in the same zone. Hosts that
    /// cannot answer return `0` and render in UTC; `pyreal_client_runtime::hud::HudView` implements it off
    /// `pyreal_desktop::platform::local_utc_offset_secs`.
    ///
    /// The default is `0` rather than a panic because a screen must draw for a host that knows
    /// nothing — [`EmptyGameView`] is exactly that host.
    fn utc_offset_secs(&self) -> crate::ctime::UtcOffsetSecs {
        0
    }

    /// Vendor-window state. The default is a closed shop, which a host
    /// without an open vendor returns and the panel must draw as hidden.
    fn shop(&self) -> ShopView {
        ShopView::default()
    }

    /// Secure-trade state. The default is a closed window, as for a host
    /// without an open negotiation. The model is `pyreal_client_model::trade`;
    /// `pyreal_client_runtime::trade_view::trade` converts it.
    fn trade(&self) -> TradeView {
        TradeView::default()
    }

    /// Ask quietly whether `id` is acceptable to the vendor — **the drag cursor's
    /// question**, and the only thing the sell list's drag-over asks.
    ///
    /// It cannot be answered from [`Self::shop`]: a `ShopView` describes the three lists, and the
    /// object being dragged is in the *pack*. The decision needs ownership by the player,
    /// the contained-item count and the vendor profile's acceptability test against the open
    /// shop's own profile, which the game world's drag-acceptance query provides.
    ///
    /// The default is **false**, which is the client's own answer when the weenie lookup misses —
    /// a host with no shop open shows the red circle, and a host with no vendor at all never raises
    /// the message, because the handler is bound to the sell list alone.
    ///
    /// It cannot be answered from [`Self::slot_decoration`]: the test reads the item's material
    /// type and bit `0x01000000` of `_bitfield`, neither of which is
    /// on that seam, **and** the `SalvageMultiple` character option, which is the player's and not
    /// the item's. The host answers it with
    /// `pyreal_client_model::inventory::salvage::is_item_suitable`.
    ///
    /// The default is **false**, which is the client's own answer when the weenie lookup misses:
    /// a host with no world refuses every drop rather than accepting every one.
    fn payment_lists(&self) -> crate::panels::slumlord::PaymentListsView {
        crate::panels::slumlord::PaymentListsView::default()
    }
    fn salvage_list(&self) -> crate::panels::salvage::SalvageListView {
        crate::panels::salvage::SalvageListView::default()
    }

    /// `None` until a `0x021D House_HouseProfile` arrives.
    fn slumlord(&self) -> Option<SlumlordView> {
        None
    }

    /// Chess-panel state. `None` means the host has no chess model.
    fn minigame(&self) -> Option<MiniGameView> {
        None
    }

    /// How many `0x021D House_HouseProfile` have arrived — and the edge that
    /// **raises** the window: the slumlord window's house-profile notice ends in
    /// making it visible, through the element itself rather than by name.
    ///
    /// A **count**, not a flag, for the reason [`Self::house_data_notices`] is one: two profiles
    /// in one frame are two house updates in retail, and a second use of the same slumlord
    /// must re-raise a window the player closed.
    fn slumlord_notices(&self) -> u64 {
        0
    }

    /// The item's public weenie class id, used by the window's remaining-payment
    /// check and by each payment entry it adds.
    fn item_wcid(&self, _item: ObjectId) -> u32 {
        0
    }

    /// Where `object` may be used from, its useability word's source half (usable at a distance,
    /// while viewed, from a container, wielded, on itself); `None` when the object or the word is
    /// not known.
    fn useability(&self, _object: ObjectId) -> Option<u32> {
        None
    }

    /// The selected object: the vendor button handler reads it once before
    /// dispatching, and eight of its eleven cases use it.
    fn selected_object(&self) -> Option<ObjectId> {
        None
    }

    /// Is `id` anywhere in the allegiance **hierarchy the shard sent**, at any depth?
    ///
    /// [`Self::allegiance_roster`] cannot answer this: it carries the monarch, the patron and the
    /// player's own vassals, which is what the panel *draws*, while `GetData` walks the whole
    /// tree. The allegiance panel needs the whole tree: the Swear
    /// button is offered only for a selected player who is **not already a member** — so this is
    /// its own question rather than a derivation of the roster.
    fn allegiance_has_member(&self, _id: ObjectId) -> bool {
        false
    }

    /// The player's `InstanceID` quality **26 (`Monarch`)**, which is what the allegiance
    /// panel's quality-changed handler watches.
    ///
    /// Its post-init registers two handlers on the player,
    /// for group 7, quality `0x19` and quality `0x1A` — group 7 is the
    /// `InstanceID` table and `0x19` / `0x1A` are `Patron` (25) and `Monarch` (26).
    ///
    /// The recorded captures confirm the direction: every `0x02DA Qualities_UpdateInstanceID` with
    /// key `0x1A` on the recorded character is followed within milliseconds by a `0x001F`.
    ///
    /// **Only the monarch half is available here**: `pyreal_client_model::weenie::mirror_stat_update` has an
    /// arm for key 26 and none for key 25, so a patron change that does not also change the
    /// monarch is invisible to this build. That is a narrower trigger than retail's, not a
    /// different one, and it is stated rather than papered over.
    fn allegiance_monarch_quality(&self) -> Option<ObjectId> {
        None
    }

    /// The component tracker's seven category lists joined to the player's desired quantities.
    ///
    /// The spell-component panel walks all seven tracker categories in order, emitting
    /// a header per category and a row per component. Each tracker list is already
    /// sorted by object name on insertion. Neither the panel nor this seam sorts it.
    ///
    /// The default is **seven empty categories, not an empty vector**: a character with no
    /// components still sees seven headers, and a host that has not filled the tracker must not be
    /// indistinguishable from one whose player carries nothing.
    fn spell_components(&self) -> Vec<ComponentCategory> {
        (0..7)
            .map(|category| ComponentCategory {
                category,
                rows: Vec::new(),
            })
            .collect()
    }

    /// The enchantment registry's cooldown test on the **player's own** registry, for the
    /// cooldown id an item carries.
    ///
    /// ```text
    /// for each entry e in the cooldown list:
    ///     if ((e.id & 0xFFFF) == key) {
    ///         remaining = (e.duration + e.start_time) - now;
    ///         if (remaining <= 0) { remove e; return 0; }
    ///         return 1;
    ///     }
    /// return 0;
    /// ```
    ///
    /// `key` is what the caller passes, and the item slot's cooldown display passes
    /// **the cooldown id `+ 0x8000`** — the offset that puts an item cooldown above the spell-id
    /// range the registry's spell totals count. Adding it is the *host's* job here, exactly as
    /// it is the caller's in the client, so the widget hands over the raw cooldown id.
    ///
    /// `now` is the frame clock, which reaches an item slot as `UiSystem::now`. It is a
    /// parameter rather than something the host samples for itself because every other value on
    /// this seam is a fact about a frozen world and this one is not: it changes between two calls
    /// in the same frame, and a cached copy would make the wedge jitter.
    ///
    /// `None` is the client's `return 0` — no registry, no such cooldown, or one that has expired.
    fn cooldown_remaining(&self, _cooldown_id: u32, _now: f64) -> Option<f64> {
        None
    }

    /// The seconds left on the cooldown an item carries, for as long as the player's registry
    /// holds it: until the server removes it, which is when the item can be used again. Past
    /// the cooldown's length by `now` and not yet removed, it is zero or less.
    ///
    /// The cooldown id is the item's own, as for [`Self::cooldown_remaining`]. `None` when the
    /// registry holds no such cooldown.
    fn cooldown_held(&self, _cooldown_id: u32, _now: f64) -> Option<f64> {
        None
    }

    /// The `COMBAT_MODE` the toolbar's stance icon shows.
    ///
    /// The default is `NONCOMBAT_COMBAT_MODE`, which is what a character enters the world in and
    /// what retail's in-game screen shows on entry; it is deliberately not `UNDEF_COMBAT_MODE`,
    /// because with `UNDEF` all four buttons are hidden and the toolbar has a hole in it.
    fn combat_mode(&self) -> u32 {
        crate::combat_mode::NONCOMBAT
    }

    /// The combat window's three read-back notices, as one snapshot. See
    /// [`CombatBar`], and `crate::hud::combat_window::CombatWindow::update`, which applies it.
    ///
    /// The default is the client's reset state, so a host that does not implement this
    /// leaves the window showing a medium attack height and a half-full notch, which is what a
    /// character who has just logged in shows.
    fn combat_bar(&self) -> CombatBar {
        CombatBar::default()
    }
}

/// A `GameView` that knows nothing, for constructing a screen with no world attached.
///
/// This is what the acceptance gate builds every panel against before it feeds one the frozen
/// snapshot: a panel that cannot be constructed against an empty world has state it should not
/// have.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyGameView;
impl GameView for EmptyGameView {}
