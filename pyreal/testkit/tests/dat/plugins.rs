//! Plugins over the retail data files: what a plugin reads of the game's own data for its maps.
//!
//! `ALL` is this file's own list, concatenated with the other subjects' in `census.rs`.

use std::sync::Arc;

use pyreal_dat::RetailDatStore;
use pyreal_plugin_host::Package;
use pyreal_testkit::claims::note_asserted;
use pyreal_testkit::plugins::PluginRun;

pyreal_testkit::scenarios! {
    scenario_a_plugin_reads_a_landblock_and_a_dungeon_cell_of_the_game_data => a_plugin_reads_a_landblock_and_a_dungeon_cell_of_the_game_data ["plugins.data.a-plugin-reads-a-landblock-and-a-dungeon-cell-of-the-game-data"],
}

/// The retail data files, as the plugin host reads them.
#[derive(Debug)]
struct Store(Arc<RetailDatStore>);

impl pyreal_plugin_host::gamedata::PluginGameData for Store {
    fn read_cell(&self, id: u32) -> Option<Vec<u8>> {
        self.0.read_cell(pyreal_primitives::DataId(id)).ok()
    }
    fn read_portal(&self, id: u32) -> Option<Vec<u8>> {
        self.0.read_portal(pyreal_primitives::DataId(id)).ok()
    }
}

/// Holtburg's landblock (`0xA9B4`): heights in metres on its 9 by 9 vertices, buildings and the
/// cells inside them; a dungeon's first cell (`0x01A40100`): its portals to the cells beside it,
/// the cells seen from it, its floor's outline and its box.
fn a_plugin_reads_a_landblock_and_a_dungeon_cell_of_the_game_data() {
    let dir = pyreal_dat::testing::dat_dir();
    let store = Arc::new(RetailDatStore::open_dir(&dir).unwrap_or_else(|e| {
        panic!(
            "this scenario needs the retail dats under $PYREAL_TEST_DAT_DIR ({}): {e}",
            dir.display()
        )
    }));
    let main = r#"
local lb = game.data.landblock(0xA9B4)
local hmin, hmax = math.huge, -math.huge
for _, h in ipairs(lb.heights) do
	hmin, hmax = math.min(hmin, h), math.max(hmax, h)
end
print("landblock", #lb.heights, #lb.terrain, lb.buildings > 0, #lb.cells > 0, hmax > hmin)
local d = game.data.landblock(0x01A4)
local c = game.data.cell(d.cells[1])
local floor = 0
for _, p in ipairs(c.floor) do
	floor = floor + #p
end
print("cell", string.format("0x%08X", c.id), #c.portals > 0, #c.visible > 0, floor > 0,
	c.bounds.max.x > c.bounds.min.x)
print("none", game.data.landblock(0xFFFF) == nil, game.data.cell(0x01A4FFF0) == nil)
"#;
    let package = Package {
        origin: "maps".into(),
        files: [
            (
                "plugin.toml".to_owned(),
                b"id = \"test.maps\"\nname = \"Maps\"\nversion = \"1.0.0\"\napi = \"0.6\"\ncontexts = [\"pregame\", \"world\"]\n"
                    .to_vec(),
            ),
            ("main.lua".to_owned(), main.as_bytes().to_vec()),
        ]
        .into(),
    };
    let mut run = PluginRun::new().with_game_data(Box::new(Store(store)));
    run.load(package);
    run.frame();
    assert_eq!(
        run.console("test.maps"),
        [
            "landblock\t81\t81\ttrue\ttrue\ttrue",
            "cell\t0x01A40100\ttrue\ttrue\ttrue\ttrue",
            "none\ttrue\ttrue",
        ]
    );
    note_asserted("plugins.data.a-plugin-reads-a-landblock-and-a-dungeon-cell-of-the-game-data");
}
