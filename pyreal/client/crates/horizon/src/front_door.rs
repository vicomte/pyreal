//! The screens before any world, as the host hands them over: the player's worlds, their data
//! files and what the last request came to ([`FrontDoorView`]), and what the player asked of
//! them ([`FrontDoorRequest`]), for the host to carry out. The host keeps the worlds, the
//! accounts and the passwords; the interface only shows them.

/// One world on the player's list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorldRow {
    /// Its id among the worlds.
    pub slug: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    /// The account last played there.
    pub account: Option<String>,
    /// Whether that account's password is kept.
    pub password_saved: bool,
}

/// One world of the list of known worlds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListedRow {
    /// Its id in the list.
    pub slug: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    /// The server software it runs, by name.
    pub emulator: String,
    /// Its rules (`PvE` or `PvP`), when the list says.
    pub ruleset: Option<String>,
    /// How settled it is (`Stable`, `Development`), when the list says.
    pub maturity: Option<String>,
    pub description: Option<String>,
    /// Whether it is on the player's list already.
    pub added: bool,
}

/// The list of known worlds, as the host last read it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DirectoryView {
    pub worlds: Vec<ListedRow>,
    /// Whether the list is being read now.
    pub reading: bool,
    /// Where and when the copy shown was read, or why there is none, in words.
    pub note: Option<String>,
}

/// The data files the client plays with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DatsView {
    /// The folder of the four later files.
    pub modern: Option<String>,
    /// The folder of the February 2005 `portal.dat` and `cell.dat`, when one was chosen.
    pub classic: Option<String>,
    /// The folder of the 1999 `portal.dat` and `cell.dat`, when one was chosen.
    pub original: Option<String>,
    /// Whether the later files are there and read.
    pub ready: bool,
    /// What is wrong with the folders, in words.
    pub problem: Option<String>,
}

/// A newer Pyreal the host found, shown on the world list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateNotice {
    pub version: String,
}

/// What the host hands the screens before any world, each frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrontDoorView {
    pub worlds: Vec<WorldRow>,
    /// The world to select first: the one last played.
    pub last_world: Option<String>,
    pub dats: DatsView,
    /// What the last request came to, when it went wrong, or why the last session ended.
    pub message: Option<String>,
    /// Whether the host can show the system's folder picker.
    pub can_browse: bool,
    /// The list of known worlds.
    pub directory: DirectoryView,
    /// A newer release to tell the player about.
    pub update: Option<UpdateNotice>,
}

/// What the player asked of the screens before any world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrontDoorRequest {
    /// Add a world (`slug` none) or change one: its name, host and port as typed.
    SaveWorld {
        slug: Option<String>,
        name: String,
        host: String,
        port: String,
    },
    RemoveWorld {
        slug: String,
    },
    /// Move a world one place up the list (`up`) or down it.
    MoveWorld {
        slug: String,
        up: bool,
    },
    /// Add a world from the list of known worlds, by its id there.
    AddListedWorld {
        slug: String,
    },
    /// Read the list of known worlds: when the copy is old or missing, or always when `again`.
    ReadDirectory {
        again: bool,
    },
    /// Log in to a world: the account, and the password typed (empty for the one kept), kept
    /// for next time when `remember`.
    SignIn {
        slug: String,
        account: String,
        password: String,
        remember: bool,
    },
    /// Forget the kept password of an account on a world.
    ForgetPassword {
        slug: String,
        account: String,
    },
    /// Play with the data files in these folders: the later set's, the 2005 pair's and the 1999
    /// pair's (empty for none).
    SetDats {
        modern: String,
        classic: String,
        original: String,
    },
    /// Pick a folder with the system's picker.
    BrowseDats {
        folder: DatsFolder,
    },
    /// End the session and go back to the world list.
    LogOut,
    /// Fly through the files in one of the data files' folders, with no server.
    Explore {
        folder: DatsFolder,
    },
    /// End explore mode and go back to the world list.
    EndExplore,
    /// Open the newer release's page in the browser.
    OpenUpdate,
}

/// One of the data files' folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatsFolder {
    /// The later files, from Throne of Destiny on.
    Modern,
    /// The February 2005 `portal.dat` and `cell.dat`.
    Classic,
    /// The 1999 `portal.dat` and `cell.dat`.
    Original,
}
