//! Pyreal's admin commands as a world speaking the November 1999 client's protocol takes them.
//!
//! The AC1999 project's server has its own commands (its `@emuhelp` lists them by group), named
//! and shaped differently from the later servers' that [`super::admin`] lists: `@tele` takes a
//! player there and `@teleto` coordinates, the other way round from here. [`admin_on_1999`] gives
//! the server's form of a line, or says it has none.

use super::admin::admin_command;

/// What a line becomes on a 1999 world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum On1999 {
    /// It goes as it is: the server's own command, one the client answers itself, or one neither
    /// list knows, which the server answers.
    Same,
    /// The server's form of it, one line or several.
    Lines(Vec<String>),
    /// The server has nothing that does it.
    Missing,
}

/// The AC1999 server's command words, from its command tables and help commands.
pub const AC1999_COMMANDS: &[&str] = &[
    "activeevents",
    "addmodelswap",
    "addspellbyid",
    "addspelltoitem",
    "addtitle",
    "adminvision",
    "allegbreak",
    "allegdump",
    "animation",
    "animationall",
    "animationother",
    "arwic",
    "attackable",
    "barber",
    "blueproc",
    "cast",
    "characterhelp",
    "chelp",
    "cleanhousing",
    "clearquests",
    "clearspawns",
    "cloakbaseproc",
    "cloakhalfhealthproc",
    "cloakperlevelproc",
    "cloakquarterhealthproc",
    "cloaktenthhealthproc",
    "comptrack",
    "damagesources",
    "debug",
    "debugresistances",
    "decent",
    "decquest",
    "doomshard",
    "dropitemsondeath",
    "dungeon",
    "effect",
    "ehelp",
    "emuhelp",
    "enddot",
    "envmode",
    "erasequest",
    "evict",
    "explorehelp",
    "exportrecipe",
    "freecam",
    "freezeall",
    "g_disable",
    "g_enable",
    "generalhelp",
    "getcreditother",
    "getemail",
    "getinfo",
    "ghelp",
    "givecredit",
    "givecreditother",
    "givelum",
    "givexp",
    "givexpother",
    "global",
    "godly",
    "importrecipe",
    "incquest",
    "instakill",
    "invis",
    "lbe",
    "learnschool",
    "loadjsonwcid",
    "loc_other",
    "monsterbrawl",
    "motion",
    "movetome",
    "myloc",
    "myquests",
    "npk",
    "nudge_x",
    "nudge_y",
    "nudge_z",
    "palbase",
    "pk",
    "player",
    "playsound",
    "pop",
    "portalstorm",
    "purgespells",
    "radar",
    "redproc",
    "reloadevents",
    "reloadlb",
    "reloadquests",
    "reloadtreasure",
    "reloadtreasuretables",
    "removedid",
    "removefloat",
    "removeiid",
    "removeint",
    "removeint64",
    "removethis",
    "researchtable",
    "resethousepurchasetimestamp",
    "resetlb",
    "restorechar",
    "restoremychar",
    "serverhelp",
    "serverstatus",
    "setattr",
    "setbool",
    "setdid",
    "setemail",
    "setfloat",
    "sethealth",
    "setiid",
    "setint",
    "setint64",
    "setname",
    "setplayer",
    "setprefix",
    "setquest",
    "setskill",
    "setsolid",
    "setstamina",
    "setstring",
    "setvital",
    "setvoidconfig",
    "showmodel",
    "simulateaccess",
    "skillspendexp",
    "sound",
    "spawnbael",
    "spawndot",
    "spawnfollow",
    "spawnfollow2",
    "spawnfresh",
    "spawnhelp",
    "spawnsalvagewcid",
    "spawnset",
    "spawntreasure",
    "spawnwand",
    "spawnwcid",
    "spawnwcidfresh",
    "spawnwcidinv",
    "spawnwcidinvfresh",
    "spawnwcidpts",
    "spellcomps",
    "spelleconomy",
    "sphelp",
    "spin",
    "squelchall",
    "stampquest",
    "startevent",
    "stopevent",
    "svhelp",
    "sweartime",
    "targetdrudge",
    "targeteffect",
    "targetsound",
    "tele",
    "teleadmin",
    "teleloc",
    "telelocd",
    "teleto",
    "teletome",
    "teletown",
    "testchat",
    "timeadjust",
    "usecomponents",
    "vismode",
    "waivenextrent",
    "wcidbodyall",
    "wdaddmodelswap",
    "wdsetattr",
    "wdsetbool",
    "wdsetdid",
    "wdsetfloat",
    "wdsetiid",
    "wdsetint",
    "wdsetint64",
    "wdsetskill",
    "wdsetstring",
    "wdsetvital",
    "welcomemessage",
    "wiflag",
    "xp",
    "yellowproc",
];

/// The server's command lists, one help command per group, for an administrator to read.
pub const AC1999_HELP: [(&str, &str); 5] = [
    ("General", "@generalhelp"),
    ("Travel", "@explorehelp"),
    ("Spawning", "@spawnhelp"),
    ("Character", "@characterhelp"),
    ("Server", "@serverhelp"),
];

/// How far in front of the administrator `@spawnwcid` puts what it makes, in meters, as the
/// later servers' `@create` does.
const SPAWN_AHEAD: &str = "2";

/// Whether `verb` is one of the AC1999 server's commands.
#[must_use]
pub fn is_ac1999_command(verb: &str) -> bool {
    AC1999_COMMANDS.iter().any(|c| c.eq_ignore_ascii_case(verb))
}

/// A value as one argument: in double quotes when it has spaces, which the server's line splits
/// on. A value already quoted stays as it is.
fn one_argument(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        return value.to_owned();
    }
    let words: Vec<&str> = value.split_whitespace().collect();
    let value = words.join(" ").replace('"', "");
    if value.contains(' ') {
        format!("\"{value}\"")
    } else {
        value
    }
}

/// One coordinate: a number with its direction after it, as `37.3n` or `67W`.
fn coordinate(word: &str, directions: [char; 2]) -> Option<String> {
    let (at, direction) = word.char_indices().last()?;
    let number = &word[..at];
    let direction = direction.to_ascii_uppercase();
    (directions.contains(&direction) && number.parse::<f64>().is_ok_and(f64::is_finite))
        .then(|| format!("{number}{direction}"))
}

/// Map coordinates, north or south first: `37.3n,67.0w`, `37.3N, 67W` or `37.3n 67.0w`.
fn coordinates(text: &str) -> Option<(String, String)> {
    let words: Vec<&str> = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|w| !w.is_empty())
        .collect();
    let [ns, ew] = words.as_slice() else {
        return None;
    };
    Some((coordinate(ns, ['N', 'S'])?, coordinate(ew, ['E', 'W'])?))
}

/// A line's form on a 1999 world. `line` is a command as typed or sent, `@` or `/` first.
#[must_use]
pub fn admin_on_1999(line: &str) -> On1999 {
    let line = line.trim();
    let Some(body) = line.strip_prefix('@').or_else(|| line.strip_prefix('/')) else {
        return On1999::Same;
    };
    let (verb, rest) = body.split_once(' ').unwrap_or((body, ""));
    let rest = rest.trim();
    let verb = verb.to_ascii_lowercase();
    let one = |l: String| On1999::Lines(vec![l]);
    let words: Vec<&str> = rest.split_whitespace().collect();
    let mapped = match verb.as_str() {
        // Coordinates go to `@teleto`; a name stays, as the server's `@tele` takes a player.
        "tele" => match coordinates(rest) {
            Some((ns, ew)) => one(format!("@teleto {ns} {ew}")),
            None if words.len() > 1 && coordinates(&words[1..].join(" ")).is_some() => {
                On1999::Missing
            }
            None if rest.is_empty() => On1999::Same,
            None => one(format!("@tele {}", one_argument(rest))),
        },
        // The later servers' `@teleto` takes a player there, which is the server's `@tele`.
        "teleto" => match coordinates(rest) {
            Some((ns, ew)) => one(format!("@teleto {ns} {ew}")),
            None if rest.is_empty() => On1999::Same,
            None => one(format!("@tele {}", one_argument(rest))),
        },
        "teletome" if !rest.is_empty() => one(format!("@teletome {}", one_argument(rest))),
        "cloak" => match rest.to_ascii_lowercase().as_str() {
            "on" | "off" => one(format!("@invis {}", rest.to_ascii_lowercase())),
            _ => On1999::Missing,
        },
        // The server reads a number.
        "adminvision" => match rest.to_ascii_lowercase().as_str() {
            "on" | "1" => one("@adminvision 1".to_owned()),
            "off" | "0" => one("@adminvision 0".to_owned()),
            _ => On1999::Missing,
        },
        "god" => one("@godly".to_owned()),
        // The server's `@pk` takes nothing; `@npk` is a command of its own.
        "pk" => match rest.to_ascii_lowercase().as_str() {
            "" => On1999::Same,
            "pk" => one("@pk".to_owned()),
            "npk" => one("@npk".to_owned()),
            _ => On1999::Missing,
        },
        "addallspells" => On1999::Lines(
            ["war", "life", "item", "creature"]
                .iter()
                .map(|s| format!("@learnschool {s}"))
                .collect(),
        ),
        "delete" if rest.is_empty() => one("@removethis".to_owned()),
        "targetloc" => one("@loc_other".to_owned()),
        // The later `@create <what> [amount] [palette] [shade]`; the server's takes a distance
        // before the palette.
        "create" if !words.is_empty() => {
            let amount = words.get(1).copied().unwrap_or("1");
            let mut l = format!("@spawnwcid {} {amount} {SPAWN_AHEAD}", words[0]);
            for w in words.iter().skip(2) {
                l.push(' ');
                l.push_str(w);
            }
            one(l)
        }
        "ci" if !words.is_empty() => one(format!("@spawnwcidinv {rest}")),
        "comps" => one("@spellcomps".to_owned()),
        "gamecast" if !rest.is_empty() => one(format!("@global {}", one_argument(rest))),
        "reload-landblock" => one("@reloadlb".to_owned()),
        "event" => match words.as_slice() {
            ["start", name] => one(format!("@startevent {name}")),
            ["stop", name] => one(format!("@stopevent {name}")),
            _ => On1999::Missing,
        },
        // The 1999 client's own `@save`, which ties the character to the nearest lifestone.
        "save" if rest.is_empty() => On1999::Same,
        // Answered by the client itself.
        "loc" => On1999::Same,
        v if is_ac1999_command(v) => On1999::Same,
        v if admin_command(v).is_some() || matches!(v, "home" | "save" | "listplayers") => {
            On1999::Missing
        }
        _ => On1999::Same,
    };
    match mapped {
        On1999::Lines(lines) if lines.len() == 1 && lines[0] == line => On1999::Same,
        other => other,
    }
}

/// The lines to send for `line` on a 1999 world: its form there, or itself when it has the same
/// form or none.
#[must_use]
pub fn lines_on_1999(line: &str) -> Vec<String> {
    match admin_on_1999(line) {
        On1999::Lines(lines) => lines,
        On1999::Same | On1999::Missing => vec![line.to_owned()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(line: &str) -> Vec<String> {
        match admin_on_1999(line) {
            On1999::Lines(l) => l,
            other => panic!("{line}: {other:?}"),
        }
    }

    #[test]
    fn a_map_teleport_goes_to_teleto_with_two_coordinates() {
        assert_eq!(lines("@tele 37.3n,67.0w"), ["@teleto 37.3N 67.0W"]);
        assert_eq!(lines("@tele 0.5s, 12E"), ["@teleto 0.5S 12E"]);
        assert_eq!(lines("/tele 37.3N 67.0W"), ["@teleto 37.3N 67.0W"]);
        assert_eq!(
            admin_on_1999("@teleto 37.3N 67.0W"),
            On1999::Same,
            "the server's own form"
        );
        assert_eq!(admin_on_1999("@tele Bob 37.3N 67.0W"), On1999::Missing);
        assert_eq!(
            admin_on_1999("@tele Caf\u{e9}"),
            On1999::Same,
            "a name, whatever its last letter"
        );
    }

    #[test]
    fn a_teleport_to_a_player_goes_to_tele_with_the_name_as_one_argument() {
        assert_eq!(
            admin_on_1999("@tele Bob"),
            On1999::Same,
            "the server's own form"
        );
        assert_eq!(lines("@tele Baron Lark"), ["@tele \"Baron Lark\""]);
        assert_eq!(lines("@teleto Baron Lark"), ["@tele \"Baron Lark\""]);
        assert_eq!(lines("@teleto Bob"), ["@tele Bob"]);
        assert_eq!(lines("@teletome Baron  Lark"), ["@teletome \"Baron Lark\""]);
        assert_eq!(admin_on_1999("@teletome Bob"), On1999::Same);
    }

    #[test]
    fn what_the_server_has_under_another_name_is_renamed() {
        assert_eq!(lines("@god"), ["@godly"]);
        assert_eq!(lines("@cloak on"), ["@invis on"]);
        assert_eq!(lines("@adminvision off"), ["@adminvision 0"]);
        assert_eq!(lines("@pk npk"), ["@npk"]);
        assert_eq!(lines("@pk pk"), ["@pk"]);
        assert_eq!(lines("@delete"), ["@removethis"]);
        assert_eq!(lines("@targetloc"), ["@loc_other"]);
        assert_eq!(lines("@comps"), ["@spellcomps"]);
        assert_eq!(lines("@reload-landblock"), ["@reloadlb"]);
        assert_eq!(lines("@gamecast Hello all"), ["@global \"Hello all\""]);
        assert_eq!(lines("@event start fishing"), ["@startevent fishing"]);
        assert_eq!(lines("@addallspells").len(), 4);
    }

    #[test]
    fn making_things_takes_the_servers_spawn_commands() {
        assert_eq!(lines("@create 300"), ["@spawnwcid 300 1 2"]);
        assert_eq!(lines("@create 300 5"), ["@spawnwcid 300 5 2"]);
        assert_eq!(lines("@create 300 5 12 0.5"), ["@spawnwcid 300 5 2 12 0.5"]);
        assert_eq!(lines("@ci 300 5"), ["@spawnwcidinv 300 5"]);
    }

    #[test]
    fn what_the_server_lacks_is_missing_and_its_own_commands_and_unknown_words_go_as_they_are() {
        for line in [
            "@buff",
            "@heal",
            "@home",
            "@home 3",
            "@save 3",
            "@telepoi Arwic",
            "@grantxp Bob 100",
            "@boot char Bob",
            "@cloak player",
            "@listplayers",
            "@createnamed 300 1 Thing",
            "@event status fishing",
        ] {
            assert_eq!(admin_on_1999(line), On1999::Missing, "{line}");
        }
        for line in [
            "@save",
            "@loc",
            "@myloc",
            "@teleloc 0x7F0401AD [12.3 -28.4 0.0]",
            "@attackable on",
            "@getinfo",
            "@movetome",
            "@emuhelp",
            "@acehelp",
            "hello there",
        ] {
            assert_eq!(admin_on_1999(line), On1999::Same, "{line}");
        }
    }

    #[test]
    fn a_line_already_in_the_servers_form_keeps_it() {
        for line in [
            "@tele 37.3n,67.0w",
            "@teleto Baron Lark",
            "@god",
            "@cloak off",
            "@create 300 5 12",
            "@gamecast Hello all",
        ] {
            for out in lines_on_1999(line) {
                assert_eq!(admin_on_1999(&out), On1999::Same, "{line} -> {out}");
            }
        }
        assert!(AC1999_COMMANDS.windows(2).all(|w| w[0] < w[1]), "sorted");
    }
}
