//! The plugins the player installs, as the Plugins screen shows them ([`PluginShelfView`]), and
//! what the player asked of them ([`PluginShelfRequest`]), for the host to carry out. The host
//! keeps the sources, the installed zips and the folders; the interface only shows them.

/// One installed plugin, or one plugin in a folder on this computer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstalledRow {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Where it came from, in words: its source, a file, or its folder.
    pub from: String,
    /// Whether the client loads it; `None` until the client has found it.
    pub on: Option<bool>,
    /// The newer release its source lists, when one waits.
    pub update: Option<String>,
    /// Why it cannot run, when it cannot.
    pub problem: Option<String>,
    /// The folder on the list it was found in, for a plugin on this computer: removing it takes
    /// that folder off the list.
    pub folder: Option<String>,
}

/// One plugin source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceRow {
    pub url: String,
    pub name: String,
    /// How many plugins its last list named.
    pub plugins: usize,
    /// Why its list could not be read last time.
    pub error: Option<String>,
}

/// One plugin a source lists that is not installed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AvailableRow {
    pub source: String,
    pub source_name: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    /// Why it cannot be installed here, when it cannot.
    pub problem: Option<String>,
}

/// What the host hands the Plugins screen, each frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginShelfView {
    pub installed: Vec<InstalledRow>,
    pub sources: Vec<SourceRow>,
    pub available: Vec<AvailableRow>,
    /// What the last request came to, or what the host is doing.
    pub status: Option<String>,
    /// A download is under way: the requests that fetch wait.
    pub busy: bool,
    /// Whether the host can show the system's file picker.
    pub can_browse: bool,
}

/// What the player asked of the plugins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginShelfRequest {
    /// Load the plugin or not.
    SetOn {
        id: String,
        on: bool,
    },
    /// Remove an installed plugin and its zip.
    Remove {
        id: String,
    },
    /// Take a folder on this computer off the list; nothing in it is touched.
    RemoveFolder {
        folder: String,
    },
    /// Pick a plugin package's zip with the system's picker and install it.
    InstallFile,
    /// Pick a plugin's folder (or a folder of plugins) with the system's picker and add it.
    AddFolder,
    /// Add the source at this address, and read its list.
    AddSource {
        url: String,
    },
    RemoveSource {
        url: String,
    },
    /// Read every source's list again, and apply the updates that apply on their own.
    Refresh,
    /// Install a plugin a source lists.
    Install {
        source: String,
        id: String,
    },
    /// Update an installed plugin from its source.
    Update {
        id: String,
    },
}
