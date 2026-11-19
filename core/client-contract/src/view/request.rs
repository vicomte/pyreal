//! The request channel out of the interface, and the small values its requests carry.

use super::*;

/// What the toolbar's selection-changed handler asks the game about the selected object
/// before it decides which vital query to send.
///
/// For a selection with fewer than two items in its stack, a **player**, a **pet**, or an
/// attackable object prompts a health query. A player or pet takes that branch without consulting
/// the attackability predicate. Other objects prompt a mana query only when the local player owns
/// them. All four facts remain separate because each affects this decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SelectionQueryFacts {
    /// The is-player test — `_bitfield` bit 3.
    pub is_player: bool,
    /// Whether the object has a nonzero pet owner.
    pub has_pet_owner: bool,
    /// The combat system's is-attackable test.
    pub attackable: bool,
    /// The is-owned-by-the-player test.
    pub owned_by_player: bool,
}

/// A `UserPreferences` value, using one of the four registered data types.
#[derive(Debug, Clone, PartialEq)]
pub enum PrefValue {
    Bool(bool),
    Int(i32),
    Float(f32),
    Text(String),
}

/// Which of the allegiance panel's three confirmed actions a
/// [`UiRequest::AllegianceConfirmation`] asks about.
///
/// Break and kick both send `0x001E Allegiance_BreakAllegiance`, targeting the
/// patron and selected vassal respectively. They remain separate because their
/// prompts, enable rules and target lifetimes differ: break re-reads the patron
/// when the dialog closes, while kick latches the possible victim when it opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllegianceAction {
    /// Child `0x10000263` sends action `0x001D Allegiance_SwearAllegiance` for the selected object.
    Swear,
    /// `0x10000264` → `0x001E Allegiance_BreakAllegiance` on the player's patron.
    Break,
    /// `0x10000265` → `0x001E Allegiance_BreakAllegiance` on the selected vassal.
    Kick,
}

/// The two title representations accepted by floaty chat. A command supplies a
/// literal override; authored/default `PlayerModule` rows retain both table-reference
/// ids. A literal resembling `ID_*` must therefore remain literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatWindowTitle {
    Literal(String),
    Table { string_id: u32, table_id: u32 },
}

/// What a screen asks for. The binary routes these onward; nothing in this crate ever
/// sends a network message or mutates a world.
#[derive(Debug, Clone, PartialEq)]
pub enum UiRequest {
    /// Selects the named object.
    Select(ObjectId),
    /// Use the selected object with both optional arguments zero — the toolbar's Use button **with a selection**.
    Use(ObjectId),
    /// A targeted-use left click on an item-list row. The host reads its live mode and
    /// retained targeting source; this is deliberately not Use(selected, clicked).
    ExecuteTargetItem(ObjectId),
    /// Use `item` on `target` as the client's interface for plugins does: the target taken as
    /// the selection for the use, which sends a targeted use's request at once. With
    /// `equipped` the use is a targeted use's second click, so an item the character wields is
    /// used where it is (a caster's own spell) rather than put in the pack.
    UseOn {
        item: ObjectId,
        target: ObjectId,
        equipped: bool,
    },
    /// Close the external container after use, then unregister its range handler.
    CloseExternalContainer(ObjectId),
    /// Move everything in the open ground container to the player's pack, one item at a time.
    LootAll(ObjectId),
    /// Stop the Loot All run.
    StopLootAll,
    /// Hiding the slumlord panel retires every range registration
    /// owned by this panel, including a manual close before the one-shot exit fires.
    UnregisterSlumlordRange,
    /// Hiding the book panel retires every range registration owned
    /// by the reader, including a manual close before the one-shot exit fires.
    UnregisterBookRange,
    /// Examine the selected object — the toolbar's Examine button **with a selection**.
    Examine(ObjectId),
    /// Send `0x00C8 Item_Appraise` with a **zero** object id, the
    /// appraisal **cancel**.
    ///
    /// A variant of its own rather than `Examine(ObjectId(0))`, because in retail these are two
    /// different functions and the zero means the opposite thing in each:
    ///
    /// * The UI system's examine entry point sends an examine notice for a nonzero id and otherwise
    ///   arms the examine target mode. A zero there never reaches the appraisal sender, so it
    ///   sends nothing and arms the examine **cursor** — which is what [`Self::Examine`] means.
    /// * The examination panel's spell examine sends the appraisal event **directly**, with a
    ///   literal zero, bypassing
    ///   the examine entry point. It is the only producer of a zero-id appraise in retail.
    ///
    /// Raised by the retired retail UI's `panels::examination::ExaminationPanel::examine_spell`, and only when
    /// one of the panel's two appraisal ids is set — the entry jumps straight to the
    /// fill when both are already zero, so a spell right-click with nothing in flight puts nothing
    /// on the wire.
    CancelAppraisal,
    /// Set the target mode — the toolbar's Use and Examine buttons with
    /// **nothing selected**, which is what arms the "use on…" cursor.
    ///
    /// The toolbar's element-message handler is explicit that the two are alternatives,
    /// not a pair: on `0x1000019d`, a selection is used and the handler returns; with none it sets
    /// the use target mode — and the same shape for `0x100001a5` with examine /
    /// the examine target mode. The mode is consumed by the **next** click in the
    /// world, through the target-mode executor.
    SetTargetMode(TargetMode),
    /// A completed drag. The panel does **not** move the item; it waits for
    /// `Item_ServerSaysMoveItem`: the client predicts nothing.
    DragDrop {
        item: ObjectId,
        target: DropTarget,
    },
    /// Remove the item shortcut and notify the server, raised by
    /// the begin-drag notice when a drag is picked up **out of** a shortcut
    /// slot.
    ///
    /// It is a request of its own and not a [`DropTarget`], because in the client it has no drop:
    /// The client's closing notice fires the instant the icon leaves the
    /// tile, and a release that lands nowhere the toolbar recognises leaves the shortcut removed.
    /// That is how dragging an item off the shortcut bar removes it; there is no code on retail's
    /// drop path that would do it.
    RemoveShortcut(ObjectId),
    /// Create a shortcut to the object in the first empty slot of the bar — the make-shortcut
    /// key (`0x1000010D`) on the selected object. The host runs the whole create: the
    /// eligibility and ownership gates, the sweep for an existing shortcut or a full bar (each
    /// refused with its message), then the add with server notification.
    CreateShortcut(ObjectId),
    /// Submit `text` from chat window `window` as a chat command.
    ChatEntry {
        window: u32,
        text: String,
        action: crate::chat::entry::EntryAction,
    },
    ChatLine {
        text: String,
        window: u32,
    },
    /// A chat line the client sends on the player's behalf without the player typing it (a
    /// plugin's line, a journey's recall command): carried out as [`Self::ChatLine`] is, except
    /// that it is never taken as an extension's command and is not kept in the chat entry's
    /// history.
    ChatLineOnBehalf {
        text: String,
        window: u32,
    },
    /// Tell `target` `text` (`0x0032 Communication_TalkDirect`), as a tell to the selection would.
    Tell {
        target: ObjectId,
        text: String,
    },
    /// Sends `0x01E9 Character_RequestPing`.
    ///
    /// The link-status panel's update is the **only** thing in the client that sends it, and
    /// it sends it when the panel is opened and every 120 s while it stays open. Without it the
    /// `0x01EA` answer never arrives.
    RequestPing,
    /// Set `objectId`'s inscription to `text` with event `0x00BF`, the identify window's
    /// inscription box committing what was typed into it.
    ///
    /// Raised only by the inscription box when it loses keyboard focus. The two guards in front of
    /// it live in the panel — see
    /// the retired retail UI's `panels::examination::ExaminationPanel::handle_inscription_losing_focus`.
    SetInscription {
        object: ObjectId,
        text: String,
    },
    /// `0x00AC`, one book id.
    BookAddPage {
        book: ObjectId,
    },
    /// `0x00AB`, book, page, text.
    Book(crate::book::BookAction),
    Journal(crate::journal::JournalAction),
    BookModifyPage {
        book: ObjectId,
        page: u32,
        text: String,
    },
    /// `0x00AD`, book and page.
    BookDeletePage {
        book: ObjectId,
        page: u32,
    },
    /// `0x00AE`, book and page.
    BookPageData {
        book: ObjectId,
        page: u32,
    },
    /// `0x00AA`, one book id.
    BookData {
        book: ObjectId,
    },
    /// The authored barber Apply button after native appearance generation. The host sends the
    /// exact `0x0311` body and waits for an authoritative appearance update.
    BarberFinish(BarberAppearance),
    /// Heritage-specific local player transformation script run immediately before FinishBarber.
    /// It changes particles only; the server's later object-description update owns appearance.
    BarberLocalEffect(DataId),
    /// Empyrean Earthbound/floating motion-table selection applied immediately before
    /// FinishBarber, without rebuilding the local player's setup.
    BarberLocalMotionTable(DataId),
    /// Set talk focus to row `n` — the thirteen-row talk-focus menu
    /// picked a new destination for the next line.
    ///
    /// `n` is the menu id, which is also the value switches on:
    /// 1 = public chat, 2 = a direct tell to the last speakable target, and 3..6 =
    /// a channel broadcast with channel `0x800`, `0x2000`, `0x4000`, `0x1000` respectively.
    /// 7..13 are the Turbine chat rooms.
    ///
    /// It is a **local** setting, not a message: nothing goes to the shard when the menu changes,
    /// only when the next line is typed.
    SetTalkFocus {
        focus: u32,
    },
    /// The main chat window set its "Tell to &lt;name&gt;" target, the last speakable target:
    /// the object a line typed with talk focus 2 is told to, and the one its squelch row toggles.
    /// `ObjectId(0)` is none. The window's once-a-second sweep and its start-up reset are the only
    /// writers; a new selection is adopted only by the sweep, so the target is not simply the
    /// selection.
    SetLastSpeakableTarget {
        object: ObjectId,
    },
    /// Enable or disable talk-focus row `n` — a panel on this side of
    /// the seam decided that talk-focus row `n` is (or is not) a destination the player can pick.
    ///
    /// The three producers in the allegiance panel enable focus 4 for a logged-in patron, focus 5
    /// for a logged-in monarch other than the player, and focus 6 when any vassal is logged in. The mask
    /// itself lives in `pyreal_client_model::chat::ChatState`, which this crate may not depend on,
    /// so the call crosses as a request and the host applies it; the menu row then follows
    /// through the ordinary `TalkFocusNotice` path, exactly as the chat system's own enabler
    /// broadcasts the enable-chat-target-selection notice after writing the flag.
    SetTalkFocusEnabled {
        focus: u32,
        enabled: bool,
    },
    /// Toggle all-message squelching for this character using the current server-owned list.
    /// The character is identified by id; the wire name is empty.
    ToggleCharacterSquelch(ObjectId),
    /// Add or remove a character squelch with `0x0058 Communication_ModifyCharacterSquelch`.
    /// Typed-name operations may leave `object` zero; target-menu toggles use
    /// [`UiRequest::ToggleCharacterSquelch`] instead.
    ModifyCharacterSquelch {
        object: ObjectId,
        add: bool,
        account: String,
        message_type: u32,
    },
    /// Add or remove an account squelch for `name` with event `0x0059`. This is the
    /// Squelch Account button's only message.
    ///
    /// Both producers are in `panels::squelch`: element `0x1000054C` adds the typed name;
    /// Remove element `0x10000547` removes the selected row's text when its text element
    /// has state `0x10000057`. The wire contains only the opcode, `add` dword and one
    /// narrow string. It has no object id or message type, so this variant has two
    /// fields while [`UiRequest::ModifyCharacterSquelch`] has four.
    ModifyAccountSquelch {
        add: bool,
        name: String,
    },
    /// The friends panel's add request -- the Add button, and the `@friends add`
    /// chat command's one hop.
    ///
    /// The **name**, and only the name: event `0x0018` has no id field, as the captures confirm. The
    /// 100-friend cap is *not* applied here -- it lives with the list, in
    /// `pyreal_client_model::friends::MAX_FRIENDS`, and the panel uses the same number only to grey the
    /// button.
    AddFriend {
        name: String,
    },
    /// The client's `0x10000515` arm -- the Remove button, which reads
    /// `0x10000085` off the **selected row** and sends that id
    /// (`0x0017`).
    ///
    /// An id and not a name, which is the opposite of [`UiRequest::AddFriend`] and is retail's
    /// asymmetry rather than this build's.
    RemoveFriend {
        target: ObjectId,
    },
    /// The contracts panel's element-message handler on its `0x100005DC` arm -- the **Abandon**
    /// button, which reads the selected contract id and sends the abandon-contract event
    /// (`0x0316`).
    ///
    /// The same shape as [`UiRequest::RemoveFriend`]: an id, and one send.
    ///
    /// **Two guards are the client's, and both are here rather than at the send site.**
    /// The client refuses a selected index of `-1` and refuses `contract_id == 0`, so
    /// nothing is emitted at all unless a row is selected and carries a real id.
    AbandonContract {
        contract_id: u32,
    },
    /// Apply one checkbox's new player-option value.
    ///
    /// The host changes that single bit in the received `PlayerModule` and re-packs
    /// the same module. Constructing fresh option words from this
    /// build's checkboxes would erase unmodeled bits: the second option word has bit 25
    /// set in all five recorded `0x01A1` bodies, although the original getter stops
    /// at bit 24 and no option-set mask exceeds `0x01000000`.
    ///
    /// `crate::options::character::CharacterSettingsPage::apply` emits it and
    /// `pyreal_client_runtime::interaction` consumes it.
    SetPlayerOption(PlayerOption, bool),
    /// Query object health with event `0x01BF`.
    ///
    /// The toolbar's selection-changed handler is the only thing in the client that
    /// sends it, twice: a health query for `0` in the edge block, to stop the shard sending
    /// updates about the object just deselected, and a health query on the selected object
    /// inside the **not-a-stack** arm of the stack-size-below-2 branch. The answer, `0x01C0`, is
    /// what fills the selected object's health meter — without this request the meter draws a
    /// number nobody ever supplies.
    QueryHealth(ObjectId),
    /// Set a text property at the top of the player module's gameplay-options collection, which
    /// the server keeps for the character, and send the module to the server a few seconds after
    /// the last such change (once the character has entered the world). `property` must be one
    /// the property table types as text; any other is ignored.
    SetPlayerModuleString {
        property: u32,
        value: String,
    },
    SetPreference(&'static str, PrefValue),
    /// A player-initiated screen-size change or a tagged prompt answer.
    Resolution(crate::resolution::ResolutionAction),
    /// Set one chat-window property — the per-window position,
    /// filter and opacity blob.
    SetChatWindowOption {
        window: u32,
        property: u32,
        value: i32,
    },
    /// Set chat-window property `0x1000007F` with a `Bitfield64` —
    /// the Chat Options page's per-window text-type filter.
    ///
    /// Separate from [`Self::SetChatWindowOption`] because that variant's payload is an `i32` and
    /// the filter is 64 bits wide (`ID_ChatOption_TextFilter_Society` alone is `0x100000000`), and
    /// because the receiver has to write a `BasePropertyValue::Bitfield64` rather than an
    /// `Integer` — `pyreal_protocol::property::property_type(0x1000007F)` is `Bitfield64` and a wrong
    /// type is a silent no-op in the struct-element setter.
    ///
    /// **No datagram.** The setter's tail is the player module's changed hook,
    /// which raises the gameplay-option-changed notice locally and sets the deferred-save dirty flag;
    /// there is no `0x0005` for a chat option (the player-option-changed event is only reached
    /// from the changed hook's *player option* overload).
    SetChatWindowFilter {
        window: u32,
        mask: u64,
    },
    /// Set a floating-point gameplay option by property id —
    /// `0x10000080 Option_DefaultOpacity` or `0x10000081 Option_ActiveOpacity`, the Chat Options
    /// page's two general-section sliders.
    ///
    /// The client's named-property arm writes the **top-level**
    /// gameplay-option collection, not the per-window array, which is why one pair of sliders
    /// moves all five chat windows. Same deferred-save path as
    /// [`Self::SetChatWindowFilter`]; nothing goes on the wire here either.
    SetChatOpacity {
        property: u32,
        value: f32,
    },
    /// End the character session, optionally asking first — the indicator strip's log-out button and the
    /// Game/Support page.
    EndCharacterSession {
        ask: bool,
    },
    /// Notify the UI that the stack-slider size or maximum changed.
    StackSliderChanged {
        split: u32,
        max: u32,
    },
    /// A web address in a chat line was clicked: the player is asked first, and only a yes hands
    /// it to the host to open ([`crate::chat::links`]).
    ChatLinkClicked(String),
    /// The player opened (`true`) or closed (`false`) character creation: the one game phase a UI
    /// enters by asking (`crate::pregame::GamePhase::CharacterCreation`).
    CharacterCreation(bool),
    /// Leave the game: end the character session and then shut down. Without the character session's end the account stays logged in on the server
    /// until its own timeout.
    Quit,
    /// A player-session operation the character-management screen asks for (log on, delete,
    /// restore). The screen has no session; the UI shell's request drain routes it to the host's
    /// character-action queue, in emission order.
    CharacterAction(crate::pregame::CharacterAction),
    /// What the character-generation wizard owes the session (the creation result, the
    /// post-creation log-on). Routed by the shell's drain like [`Self::CharacterAction`].
    CharGenAction(crate::pregame::CharGenAction),
    /// Train a skill with `(skill, xp)` — `0x0046 Train_TrainSkill`.
    ///
    /// Both the skill panel's "raise" and its "raise 10" send **this**
    /// message; they differ only in the amount, which is the raise-1 cost or
    /// the raise-10 cost. There is no separate "raise ten" opcode.
    TrainSkill {
        skill: u32,
        xp: u32,
    },
    /// Train a skill's advancement class with `(skill, credits)` — `0x0047`.
    ///
    /// The *train* half: the raise path falls through to the panel's train call when
    /// the skill's `_sac` is below `TRAINED`, which raises a confirmation dialog whose callback
    /// is what sends this. An interface sends it only on that question's Yes.
    TrainSkillAdvancementClass {
        skill: u32,
        credits: u32,
    },
    /// Raise an attribute with `(attribute, xp)` -- `0x0045 Train_TrainAttribute`.
    ///
    /// The attribute panel's "raise" and "raise 10" both send
    /// **this** message for a row whose token reports stat type `8`; as on the skills page the
    /// two differ only in the amount, which is the raise-1 cost or the raise-10
    /// cost. There is no "raise ten" opcode.
    TrainAttribute {
        attribute: u32,
        xp: u32,
    },
    /// Send `0x0044 Train_TrainAttribute2nd` for a vital, from the same raise-one
    /// and raise-ten paths used for primary attributes.
    ///
    /// The answer is `Qualities_PrivateUpdateAttribute2nd` (`0x02E7`), carrying the
    /// whole `SecondaryAttribute` record. It shares `PropertySequenceGate` tag 9
    /// with regeneration ticks. `pyreal_client_model::qualities::update::answers_a_raise_update`
    /// distinguishes the answer that releases the raise latch.
    TrainAttribute2nd {
        vital: u32,
        xp: u32,
    },
    /// Set the spellbook filter mask with event `0x0286`.
    ///
    /// The spellbook's filter update writes the new mask into the player module **and** sends it,
    /// and only when it actually changed.
    SetSpellbookFilter {
        mask: u32,
    },
    /// The spell bar's add-to-player-module — the request that makes
    /// magic combat reachable.
    ///
    /// The client updates the player module first and then sends event `0x01E3`. Model **and**
    /// wire move in that order from one call, which is why this is a single request
    /// and not a "move the icon" plus a "tell the shard". A spell bar is persistent server state:
    /// a client that inserted the row and sent nothing would look correct until the relog.
    ///
    /// `index` is the spell bar's own, already adjusted: a drop names the
    /// row it landed on, an append names the spell count **after** its own increment (one past the
    /// last row, which the packable list resolves to a tail push), and a move
    /// that came from earlier in the same tab has had one subtracted.
    AddSpellFavorite {
        spell_id: u32,
        index: i32,
        tab: usize,
    },
    /// The spell bar removes the favorite from the local model, then sends event `0x01E4`.
    ///
    /// Raised by the bar's remove-from-menu, which is reached two ways: the bar's own
    /// begin-drag notice (dragging a spell *off* the bar) and
    /// the first half of every add-favorite — so re-dropping a spell already on the tab sends
    /// this and then [`UiRequest::AddSpellFavorite`], which is how retail spells "move".
    RemoveSpellFavorite {
        spell_id: u32,
        tab: usize,
    },
    /// Remove the spell with event `0x01A8` — the spellbook's DELETE button.
    ///
    /// The spellbook's delete button does not remove anything: it raises a confirmation
    /// dialog carrying the selected spell id under property `0x1000003F`, and
    /// the dialog's callback sends this when `0x92` came back true. The row
    /// leaves the list when the shard echoes the removal, not when the button is pressed.
    RemoveSpell {
        spell_id: u32,
    },
    /// Notify the UI that an item list now has a different parent container.
    ///
    /// The setter broadcasts only when the parent actually changed: it checks the old
    /// id before writing and uses that result again afterward. The listener stores
    /// the open-container id, which the pickup destination prefers over the player.
    ///
    /// This is a request because the panel is inside element-message dispatch and
    /// cannot borrow the mutable world directly.
    NewParentContainer(ObjectId),

    // ---------------------------------------------------------------------------------------
    // The vendor window's button handler and its eleven `case`s, as the four
    // distinct things they do. The element ids are the cases themselves and live in
    // the retired retail UI's `panels::vendor`; a request carries what the button decided, not which button.
    // ---------------------------------------------------------------------------------------
    /// Buy a single item — cases `0x100000C2` ("Buy") and `0x100000C9`
    /// ("Buy Item"). **This sends `0x005F` on its own**, with a one-entry list; it does not fill
    /// the basket. `split` is the current stack-slider answer.
    VendorBuySingle {
        item: ObjectId,
        split: i32,
    },
    VendorAddToBuyList {
        item: ObjectId,
        split: i32,
    },
    /// Set the vendor row's object stack size -- and it is not a
    /// UI call at all.
    ///
    /// It divides the description value by the old stack size (using 1 when that size is zero),
    /// multiplies by `size`, then stores the scaled value and new stack size.
    ///
    /// This changes the local copy of a vendor stock object. Its stack size limits
    /// how much one Add to List action can cover; rescaling value by the same factor
    /// keeps the per-unit price unchanged. The Add to List handler uses the split
    /// slider amount when stack size is at least 2, otherwise 1. Without this write,
    /// a stackable row always adds one item regardless of the slider.
    ///
    /// the retired retail UI's `panels::vendor::VendorPanel::update_items_list` emits this once for
    /// each listed row whose maximum stack size exceeds 1. The host owns and changes
    /// the description.
    VendorSetObjectStackSize {
        item: ObjectId,
        size: i32,
    },
    /// A buy shop event over the whole basket — case `0x100000CA` ("Buy All").
    VendorBuyAll,
    /// Sell a single item — case `0x100000D2` ("Sell Item"), which also
    /// sends on its own.
    VendorSellSingle {
        item: ObjectId,
    },
    /// Sell the complete sell list to the current vendor — case `0x100000D3` ("Sell All"). This is
    /// the shape the corpus's one recorded `0x0060` has: **two rows in one message.**
    VendorSellAll,
    /// Remove one basket item for `0x100000CB` / `0x100000D4` (Clear Item), or the
    /// whole list for `0x100000CC` / `0x100000D5` (Clear List). `item` is `None`
    /// for the whole-list form.
    VendorClearList {
        sell: bool,
        item: Option<ObjectId>,
    },
    /// Close the vendor window — case `0x100000D6`, the close button.
    VendorClose,
    /// The sell list's drag accept — **an item dragged onto the sell list**, which
    /// is the one thing this window takes that is not a button.
    ///
    /// The vendor window's drop release is the same "dropped inside the sell list" + drop-icon
    /// info + drop-item alias shape
    /// the secure-trade window's drop release has, and this is its
    /// [`UiRequest::TradeAddItem`]. The refusals, the marks and the basket are
    /// owned by the game world's add-to-sell operation, because every one of them needs the object table:
    /// ownership by the player, the contained-item count and the vendor profile's acceptability
    /// test.
    ///
    /// **Local only.** Nothing goes on the wire until "Sell Item" or "Sell All"; the client's own
    /// drop-acceptance check sends nothing either.
    VendorAddToSell {
        item: ObjectId,
    },
    /// The sell list's drop with only part of a stack dialled in. The host asks for the split
    /// and puts the source on the list as the row's placeholder; the object the server makes
    /// takes that row when it arrives. `split` and `max` are the splitter as the drop left it.
    VendorSplitToSell {
        item: ObjectId,
        split: u32,
        max: u32,
    },

    /// The client's message `0x2F` arm — the player
    /// committed a new number in a component row's edit field `0x1000046B`.
    ///
    /// The host stores the desired component level and sends event `0x0224`. Out of range
    /// (`level < 0 || level >= 0x1389`) is not a
    /// send and the panel reverts the field, so the request is raised only for a value the panel
    /// has already bounds-checked -- the host checks again, because the bound is enforced three
    /// times in the client and this crate is not the authority on it.
    SetDesiredComponentLevel {
        wcid: u32,
        level: i32,
    },
    /// Fill the component list for a category and maximum price -- the "buy the components I
    /// am short of" helper. `category` is `None` for the client's
    /// undefined spell-component category, which means every category; `max_price` of 0 is no
    /// limit.
    VendorFillComponents {
        category: Option<u32>,
        max_price: i32,
    },

    /// Cast `spell_id` with the from-UI flag set — and
    /// the request that makes a player able to cast a spell at all.
    ///
    /// Raised by the retired retail UI's `panels::spellcasting::SpellcastingPanel::cast`, which is
    /// the spellcasting panel's cast — the **only** caller `CastSpell` has in the shipped
    /// binary. Everything the client decides after that point (the component check, the
    /// self-targeted branch, the target compatibility test and which of `0x0048` /
    /// `0x004A` goes on the wire) belongs to the game world's spell-casting operation and is deliberately
    /// not duplicated here: this crate carries the click, not the rules.
    CastSpell {
        spell_id: u32,
    },

    /// The spell research page's Test: the formula's components (component weenie class ids, in
    /// the order they were laid) tried on the selected target. See [`crate::research`].
    TestSpellFormula {
        components: Vec<u32>,
    },

    /// Put `item`, one of the player's own, on the ground: the drop-selection key and a drag out
    /// of a pack onto the world.
    PutInWorld(ObjectId),
    /// Give `amount` of `item`, one of the player's own, to `target` (a person, a creature): what
    /// a drag of it onto them asks.
    GiveTo {
        item: ObjectId,
        target: ObjectId,
        amount: u32,
    },
    /// The privileged player's map teleport: to the middle of the outdoor block at (`x`, `y`) in
    /// the 2040-block world grid.
    MapTeleport {
        x: u32,
        y: u32,
    },
    /// Ask the server for the house the player owns.
    QueryHouse,

    /// The combat system's set-requested-attack-height — the arm
    /// that makes the combat window's three attack-height buttons do anything.
    ///
    /// Raised by `crate::hud::combat_window::CombatWindow::on_element_message`'s `0x1C`
    /// (mouse-press) arm and by `pyreal_client_runtime::interaction::Interaction::on_actions`' `CombatLow/Medium/High
    /// Attack` and `CombatAimLow/Medium/High` keys, which are the same act: the combat action
    /// handler and the combat window's element-message handler call the one function.
    ///
    /// `height` is `ATTACK_HEIGHT` (`HIGH` 1, `MEDIUM` 2, `LOW` 3).
    CombatSetAttackHeight {
        height: u32,
    },

    /// End the attack at `height` with the power override set to `-1.0` — the
    /// release half. `-1.0` is "use what the bar says", which is why the button is a *held*
    /// control: press charges, release swings.
    CombatEndAttack {
        height: u32,
    },

    /// The combat window's element-message handler on its `0x0A` arm — the
    /// power/recklessness gauge. `position` is the scrollbar position in thousandths, read back
    /// unsigned exactly as the client's integer-to-float conversion does; the clamp to `[0, 1]`
    /// belongs to `pyreal_client_model::CombatState::set_ui_requested_power_from_scrollbar`.
    CombatSetDesiredPower {
        position: u32,
    },

    /// Toggle between peace and the combat mode the held equipment calls for: what the combat
    /// toggle key does, asked for by something with no key (the frame's extension hook).
    ToggleCombatMode,

    /// Stop attacking: cancel an attack sent or repeated, as the escape key does, and drop a swing
    /// still charging, asked for by something with no key (the frame's extension hook).
    CombatAbortAttack,

    // -----------------------------------------------------------------------------------------
    // Secure trade. The panel carries the clicks; the rules beneath
    // them belong to `pyreal_client_model::trade`.
    // -----------------------------------------------------------------------------------------
    /// Add an item — an item dropped on the table.
    ///
    /// `position` is the row's index in the window's **own** self list and becomes the second dword
    /// of `0x01F8 Trade_AddToTrade`. It
    /// is the panel's number because the list is the panel's.
    TradeAddItem {
        item: ObjectId,
        position: u32,
    },
    /// The partial-stack arm. The source is not inserted; these splitter values
    /// are used to request an authoritative result.
    TradeSplitItem {
        item: ObjectId,
        split: u32,
        max: u32,
    },
    /// The client's unequal-stack arm. Retail asks the
    /// inventory holder to split this many beside the source; the resulting object is selected by
    /// the ordinary authoritative create path and is not inserted into the payment list until the
    /// player drops it a second time.
    PaymentList(crate::panels::slumlord::PaymentAction),
    SalvageList(crate::panels::salvage::SalvageAction),
    HouseSplitItem {
        item: ObjectId,
        split: u32,
        max: u32,
    },
    /// Accept the trade — the trade button pressed at state **6**.
    ///
    /// The two counts are what the window is showing. They are carried rather than re-derived
    /// because the comparison against `pyreal_client_model`'s mirror **is** the client's whole share of the
    /// anti-scam protocol, and a comparison of a value with itself would assert nothing.
    TradeAccept {
        displayed_self: usize,
        displayed_partner: usize,
    },
    /// Decline the trade — the same button pressed at state **1**.
    TradeDecline,
    /// Reset the trade — the "Clear All Items" button, `0x1000008A`.
    TradeReset,
    /// The client's hide arm — the close button
    /// `0x1000008B` hides the window, and hiding it is what sends
    /// `0x01F7 Trade_CloseTradeNegotiations`.
    TradeClose,

    /// Enable or disable allegiance updates with `0x001F Allegiance_UpdateRequest`, the
    /// subscribe toggle the shard answers with `0x0020 Allegiance_AllegianceUpdate`. Without it
    /// the allegiance panel stays empty.
    ///
    /// The allegiance panel has **five** call sites for it — its post-init, its
    /// player-description notice, its quality-changed handler and both arms of its
    /// visibility change — and this build emits the visibility pair. The shard is
    /// the only source of a roster: it answers this request, or pushes on a change to an
    /// allegiance it has already been asked about. A client that never sends it sees nothing, for
    /// ever, and cannot tell that apart from belonging to no allegiance.
    ///
    /// In the three recorded fellowship sessions, where the client half is **retail's**, there
    /// are 31 `0x001F`, 28 with `on = 1` and exactly one `on = 0` per session, every one of the 31
    /// answered within milliseconds by `0x0020` and then `0x01C8`.
    AllegianceUpdateRequest {
        on: bool,
    },

    /// The allegiance panel's three confirmation dialogs: the question, before any
    /// send.
    ///
    /// The panel's element-message handler on message `1` does nothing but raise a dialog:
    ///
    /// ```text
    /// 0x10000263 -> the swear confirmation dialog
    /// 0x10000264 -> the break confirmation dialog
    /// 0x10000265 -> the kick confirmation dialog
    /// ```
    ///
    /// and the game action goes out of the *close* callback of the break and the kick dialogs
    /// rather than out of the button.
    /// So this request carries no opcode: it asks the host to put a question on screen, and the
    /// host sends `0x001D` / `0x001E` if and only if the player answers yes. A build that sent
    /// straight from the button would swear or break on one click, which retail never does.
    ///
    /// `prompt` is resolved here rather than by the host because the three
    /// `ID_Allegiance_*Confirmation` strings live in this crate's string table
    /// (the retired retail UI's `panels::allegiance::STRING_TABLE`) and the host has no reader for it.
    AllegianceConfirmation {
        action: AllegianceAction,
        target: ObjectId,
        prompt: String,
    },

    /// Set the displayed character title to `id` with event `0x002C`, four bytes of
    /// body — the Titles tab's only outbound message.
    ///
    /// The titles panel's element-message handler is the **only** caller in the
    /// image, and it is reached by one gesture: a click on the *"Set as Display Title"* button
    /// `0x10000535`, which reads `0x1000008E` off the row the list box has selected. Selecting a
    /// row sends nothing.
    ///
    /// Nothing local moves when it goes out. The worn title is server state: the answer is
    /// `0x002B Social_AddOrSetCharacterTitle` with `set_as_display_title` set, or a fresh
    /// `0x0029`, and that is what moves the header and re-enables the button.
    SetDisplayCharacterTitle {
        title_id: u32,
    },

    // -----------------------------------------------------------------------------------------
    // Fellowship UI. Every one of the seven is a fellowship event the
    // panel calls directly in the client; the request queue is this build's stand-in for that
    // call, and `pyreal_client_model::World`'s own methods hold the guards.
    //
    // The reference for all of them is the client half of the three recorded fellowship
    // sessions, which is **retail's client** and carries 27 of these.
    // -----------------------------------------------------------------------------------------
    /// Enable or disable fellowship updates with event `0x00A6`, and the whole of the fellowship
    /// panel's visibility change.
    ///
    /// The subscribe toggle for the **live vitals feed**: `0x02C0 Fellowship_UpdateFellow`
    /// arrives while it is on and stops when it is off. In the recorded three-vassal session the
    /// `on` goes out at t=259.371, the `off` at t=365.392, and all sixteen `0x02C0` of that session
    /// fall between the two.
    FellowshipUpdateRequest {
        on: bool,
    },
    /// Create the named fellowship with the requested XP-sharing state — event `0x00A2`, the Create button `0x10000274`.
    ///
    /// `share_xp` is the state of the check box on this same tab, read at
    /// the moment of the click, not a property of the fellowship being made.
    FellowshipCreate {
        name: String,
        share_xp: bool,
    },
    /// Quit or disband the fellowship with event `0x00A3`. **Both** leave buttons send it: the Quit
    /// button `0x1000027C` with `disband = false`, the Disband button `0x10000280` with `true`.
    FellowshipQuit {
        disband: bool,
    },
    /// Dismiss member `id` with event `0x00A4`, the Dismiss button `0x1000027F` on the
    /// **selected row**.
    FellowshipDismiss {
        target: ObjectId,
    },
    /// Recruit target `id` with event `0x00A5`, the Recruit button `0x1000027E` on
    /// i.e. whoever is selected **in the world**, not in the list.
    FellowshipRecruit {
        target: ObjectId,
    },
    /// Assign target `id` as leader with event `0x0290`, the Leader button `0x1000027B` on
    /// the selected row, and also the first half of a *leader's* Quit.
    FellowshipAssignNewLeader {
        target: ObjectId,
    },
    /// The change-fellowship-openness request — `0x0291`, the Open/Close button
    /// `0x1000027D`. The client flips its own `_open_fellow` first and sends the result, so the
    /// caption changes on the click; the host does the flip because the flag is `pyreal_client_model`'s.
    FellowshipToggleOpenness,
    /// The salvage window's one send: process the selected salvage operation.
    /// (`0x027D`).
    ///
    /// `items` is the window's list **reversed**: the collecting loop walks
    /// the last row down to row 0 and appends each, so the last row dropped is the
    /// first id on the wire. Carried in the request rather than re-derived at the send site,
    /// because the order is a property of *this* gesture and of the window that owns the list.
    ///
    /// Both of the client's guards are the panel's own -- a non-empty list and a
    /// non-zero tool id -- and the game world's tinkering-tool operation repeats
    /// them because a chat command could reach it without passing through the panel.
    SalvageItems {
        tool: ObjectId,
        items: Vec<ObjectId>,
    },
    /// The mini-game window's element-message handler, message-id `1` arm:
    /// one of `0x10000175` Resign, `0x10000176` Pass, `0x10000177` Stalemate.
    ///
    /// The id travels rather than a decoded intent because retail's own three arms are a
    /// subtract-and-decrement ladder on the id and each has its own guard; the decision is
    /// `pyreal_client_model::minigame::MiniGame::on_button`'s, which is the transcription of that ladder.
    MiniGameButton(u32),
    /// The game board's mouse-press handler — a left press
    /// (action `7`) on one of the board's 64 cells, as the list-box index answers it.
    /// The index→square map is the *model's*, because it needs the team.
    MiniGameBoardPress(usize),
    /// The mini-game quit dialog's callback answer —
    /// the quit-game notice's `bool`, whose Yes is the only `0x026A` in retail.
    MiniGameQuitAnswer(bool),
    /// Display `text` on `channel` -- a line straight into the scroll,
    /// which `pyreal_client_model::scroll::Scroll::on_display_string_info` already consumes.
    ///
    /// Used by the salvage panel's two channel-`0x1A` lines: the heading printed
    /// before walking a dropped pack, and "You can only salvage items that you own!"
    /// This differs from [`UiRequest::ChatLine`], which is player input parsed for
    /// slash commands before it reaches the scroll.
    DisplayChatText {
        feedback: crate::feedback::Feedback,
        channel: u32,
        text: String,
    },
    /// The slumlord window's payment request (`0x021C`).
    /// and the rent request (`0x0221`), which of the two chosen by the current house operation.
    ///
    /// `items` is the **current** item list read top to bottom (row `0` to the last
    /// row), each row's object id, skipping a zero — the
    /// opposite order from [`UiRequest::SalvageItems`], which walks its list backwards. A recorded
    /// refused house purchase shows the drop order preserved on the wire at `t = 144.458`:
    /// `3af0da79 03000000 c2150080 c1150080 92150080`.
    ///
    /// Both of the client's guards are the panel's own: an op that is `Buy` or `Rent` and a
    /// non-empty list, plus a non-zero owner id.
    HousePayment {
        slumlord: ObjectId,
        rent: bool,
        items: Vec<ObjectId>,
    },
    /// The slumlord window's buy confirmation dialog, or its
    /// rent-by-proxy confirmation dialog. The panel keeps the payment rows;
    /// the gameplay dialog owner supplies the shipped confirmation dialog and routes its answer
    /// back through [`UiRequest::HousePaymentConfirmationAnswer`].
    HousePaymentConfirmation {
        rent: bool,
    },
    /// The matching completion. Property `0x8E` identifies
    /// the dialog kind; the Boolean answer itself is property `0x92`. `None` means creation or
    /// framework teardown supplied no answer and only releases the panel's context guard.
    HousePaymentConfirmationAnswer {
        rent: bool,
        confirmed: Option<bool>,
    },

    // -----------------------------------------------------------------------------------------
    // Requests no retail panel makes: what a plugin asks for directly. Each ends in the same
    // game operation, with the same guards and the same message, as the gesture it stands for.
    // -----------------------------------------------------------------------------------------
    /// Appraise an object without opening the examination window: `0x00C8 Item_Appraise`, the
    /// request alone. The answer lands in the appraisal cache like any other.
    Appraise(ObjectId),
    /// One inventory operation, named rather than reached through a drag.
    Inventory(InventoryIntent),
}

/// An inventory operation a plugin asks for (see [`UiRequest::Inventory`]). Each is the game
/// operation a drag reaches, with its guards: one item request at a time, and none while an attack
/// is in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryIntent {
    /// Put the whole of `item` into `container` at `place` (`0x0019`).
    PutInContainer {
        item: ObjectId,
        container: ObjectId,
        place: u32,
    },
    /// Put `amount` of the stack `item` into `container` at `place` (`0x0055`).
    SplitToContainer {
        item: ObjectId,
        container: ObjectId,
        place: u32,
        amount: u32,
    },
    /// Merge `amount` of the stack `item` into the stack `target` (`0x0054`).
    Merge {
        item: ObjectId,
        target: ObjectId,
        amount: u32,
    },
    /// Drop `item` on the ground (`0x001B`).
    Drop { item: ObjectId },
    /// Give `amount` of `item` to `target` (`0x00CD`).
    Give {
        item: ObjectId,
        target: ObjectId,
        amount: u32,
    },
    /// Wield or wear `item` where it goes, as a drop on the paper doll does (`0x001A`).
    Wield { item: ObjectId },
}
