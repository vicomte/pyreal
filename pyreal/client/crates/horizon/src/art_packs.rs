//! The art packs in the player's library, as the Data Files panel and the Graphics page show them
//! ([`ArtPacksView`]), and what the player asked of them ([`ArtPackRequest`]), for the host to
//! carry out. The host keeps, opens and remembers the packs; the interface only shows them.

/// One art pack in the library.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArtPackRow {
    /// Its file (or folder), which names it to the host.
    pub path: String,
    /// The name its manifest gives it, or its file's name when that will not read.
    pub name: String,
    /// Its size in bytes.
    pub bytes: u64,
    /// Why it cannot be drawn, when it cannot.
    pub problem: Option<String>,
}

impl ArtPackRow {
    /// How a list shows it: its name and size.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{} ({})", self.name, size_words(self.bytes))
    }
}

/// A size in the words a list shows it in: megabytes, or gigabytes past a thousand.
#[must_use]
pub fn size_words(bytes: u64) -> String {
    let mb = bytes.div_ceil(1 << 20);
    if mb >= 1000 {
        #[allow(clippy::cast_precision_loss)]
        let gb = bytes as f64 / f64::from(1u32 << 30);
        format!("{gb:.1} GB")
    } else {
        format!("{mb} MB")
    }
}

/// What the host hands the interface about the art packs, each frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArtPacksView {
    /// The library's packs, in the order added.
    pub packs: Vec<ArtPackRow>,
    /// The pack drawn; none for the built-in pack alone.
    pub chosen: Option<String>,
    /// What the host is doing or what went wrong: opening a pack, or why it would not open.
    pub status: Option<String>,
    /// Whether the host can show the system's file picker.
    pub can_browse: bool,
}

impl ArtPacksView {
    /// Whether the Graphics page offers a choice: once the library holds a pack, which is drawn
    /// over the built-in pack or left out.
    #[must_use]
    pub fn offers_choice(&self) -> bool {
        !self.packs.is_empty()
    }
}

/// What the player asked of the art packs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtPackRequest {
    /// Draw the library's pack at this path, or none.
    Choose(Option<String>),
    /// Pick a pack's file with the system's picker, add it to the library and draw it.
    AddPack,
    /// Take the pack at this path out of the library; its file is left alone.
    Remove(String),
}
