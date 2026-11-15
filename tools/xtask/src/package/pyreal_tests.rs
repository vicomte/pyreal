use super::archive::{self, Member};
use super::guard::{self, Entry, Kind};
use super::headers;
use super::pyreal::{self, Asset};
use super::targets::{self, Os, Target};
use super::tests::pe;
use super::version::{self, Product};
use crate::util::workspace_root;

fn target(triple: &str) -> Target {
    targets::find_pyreal(triple).expect("a release target")
}

const WINDOWS: &str = "x86_64-pc-windows-msvc";
const LINUX: &str = "x86_64-unknown-linux-gnu";
const MAC_ARM: &str = "aarch64-apple-darwin";
const V: &str = "0.2.0";

/// Each platform's release file is named for the version and the platform, and the folder's
/// classification reads it back.
#[test]
fn release_files_are_named_for_version_and_platform_and_read_back() {
    let names: Vec<String> = targets::PYREAL_TRIPLES
        .iter()
        .map(|t| pyreal::release_file_name(target(t), V))
        .collect();
    assert_eq!(
        names,
        [
            "pyreal-0.2.0-windows-x86_64.zip",
            "pyreal-0.2.0-linux-x86_64.tar.gz",
            "Pyreal-0.2.0-macos-aarch64.dmg",
        ]
    );
    for triple in targets::PYREAL_TRIPLES {
        let t = target(triple);
        let name = pyreal::release_file_name(t, V);
        assert_eq!(pyreal::classify(&name, V), Ok(Asset::Client(t)));
        assert_eq!(
            pyreal::classify(&format!("{name}.manifest"), V),
            Ok(Asset::ManifestPart)
        );
        assert_eq!(guard::name_finding(&name), None, "{name}");
    }
    assert_eq!(pyreal::classify("SHA256SUMS", V), Ok(Asset::Index));
    assert!(pyreal::classify("latest.json", V).is_err());
    assert!(pyreal::classify("Dereth-0.2.0-macos-aarch64.app.tar.gz", V).is_err());
    assert_eq!(pyreal::archive_root(target(MAC_ARM), V), "Pyreal.app");
    assert_eq!(pyreal::archive_root(target(LINUX), V), "pyreal-0.2.0");
}

/// Every package holds the client, the tool and the notices, the Mac bundle its plist too; the
/// base pack only where one was given, where the client looks for it.
#[test]
fn a_release_holds_the_client_the_tool_and_the_notices_and_may_hold_the_base_pack() {
    let (mac, mac_optional) = pyreal::contents(target(MAC_ARM));
    assert!(mac_optional.contains(&"Contents/Resources/base-pack.dat".to_owned()));
    assert!(!mac.iter().any(|f| f.ends_with(".dat")));
    for want in [
        "Contents/MacOS/Pyreal",
        "Contents/MacOS/pyreal-plugin",
        "Contents/Resources/NOTICE.txt",
        "Contents/Info.plist",
    ] {
        assert!(mac.contains(&want.to_owned()), "{want}: {mac:?}");
    }
    assert!(mac_optional.contains(&"Contents/Resources/Assets.car".to_owned()));
    let (win, win_optional) = pyreal::contents(target(WINDOWS));
    assert!(
        win.contains(&"Pyreal.exe".to_owned()) && win.contains(&"pyreal-plugin.exe".to_owned())
    );
    assert!(win_optional.contains(&"base-pack.dat".to_owned()));
    let (linux, _) = pyreal::contents(target(LINUX));
    assert!(linux.contains(&"pyreal".to_owned()) && linux.contains(&"LICENSE".to_owned()));
    for t in [WINDOWS, LINUX, MAC_ARM] {
        let (all, _) = pyreal::contents(target(t));
        assert!(!all
            .iter()
            .any(|f| f.contains("pyreal-client") || f.contains("launcher")));
    }
}

fn file(path: &str, bytes: &[u8]) -> Entry {
    Entry::file(path, bytes)
}

fn dir(path: &str) -> Entry {
    Entry {
        path: path.to_owned(),
        kind: Kind::Dir,
        size: 0,
        head: Vec::new(),
    }
}

/// A bundle passes with exactly its files; a stray file or a data file is refused.
#[test]
fn a_bundle_scan_passes_its_own_files_and_refuses_everything_else() {
    let (required, optional) = pyreal::contents(target(MAC_ARM));
    let mut entries = vec![
        dir(""),
        dir("Contents"),
        dir("Contents/MacOS"),
        dir("Contents/Resources"),
    ];
    entries.extend(required.iter().map(|p| file(p, b"x")));
    let scan = |entries: &[Entry]| {
        guard::scan_tree(
            entries,
            &required,
            &optional,
            pyreal::FILE_CAP,
            pyreal::TOTAL_CAP,
        )
    };
    assert_eq!(scan(&entries), Vec::<String>::new());
    let mut stray = entries.clone();
    stray.push(file("Contents/Resources/extra.txt", b"x"));
    assert!(!scan(&stray).is_empty());
    let mut dat = entries.clone();
    dat.push(file("Contents/Resources/client_portal.dat", b"x"));
    assert!(!scan(&dat).is_empty());
}

/// A written Windows zip reads back under its folder and passes the scan.
#[test]
fn a_written_windows_zip_reads_back_and_passes_the_scan() {
    let w = target(WINDOWS);
    let (required, optional) = pyreal::contents(w);
    let members: Vec<Member> = required
        .iter()
        .map(|n| Member {
            name: n.clone(),
            bytes: n.as_bytes().to_vec(),
            executable: n.ends_with(".exe"),
        })
        .collect();
    let root = pyreal::archive_root(w, V);
    let bytes = archive::zip_bytes(&root, &members, 1_700_000_000).expect("zip");
    let entries = archive::relative_to_root(archive::zip_entries(&bytes).expect("reads"), &root);
    assert_eq!(
        guard::scan_tree(
            &entries,
            &required,
            &optional,
            pyreal::FILE_CAP,
            pyreal::TOTAL_CAP
        ),
        Vec::<String>::new()
    );
}

/// The bundle's programs are signed before the bundle; an identity signs under the hardened
/// runtime, with the secure timestamp when notarizing, and none signs ad hoc.
#[test]
fn the_programs_are_signed_before_the_bundle_and_an_identity_hardens_them() {
    let nested = ["Contents/MacOS/pyreal-plugin".to_owned()];
    let runs = pyreal::codesign_runs("/x/Pyreal.app", Some("ABCD"), true, &nested);
    assert_eq!(runs.len(), 2);
    assert!(runs[0].last().unwrap().ends_with("pyreal-plugin"));
    assert_eq!(runs[1].last().unwrap(), "/x/Pyreal.app");
    assert!(runs[1].windows(2).any(|w| w == ["--options", "runtime"]));
    assert!(runs.iter().all(|r| r.contains(&"--timestamp".to_owned())));
    let local = pyreal::codesign_runs("/x/Pyreal.app", Some("ABCD"), false, &nested);
    assert!(!local.iter().any(|r| r.contains(&"--timestamp".to_owned())));
    let ad_hoc = pyreal::codesign_runs("/x/Pyreal.app", None, true, &nested);
    assert!(ad_hoc.iter().all(|r| r.contains(&"-".to_owned())));
    assert!(!ad_hoc.iter().any(|r| r.contains(&"runtime".to_owned())));
}

/// A package needs no base pack, and may be given one; gathering takes nothing that builds.
#[test]
fn the_base_pack_is_a_file_package_pyreal_may_be_given() {
    let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert!(pyreal::parse_options(&args(&[])).is_ok());
    assert!(pyreal::parse_options(&args(&["--base-pack", "p.dat"])).is_ok());
    assert!(pyreal::parse_options(&args(&["--gather", "d"])).is_ok());
    assert!(pyreal::parse_options(&args(&["--gather", "d", "--base-pack", "p"])).is_err());
    assert!(pyreal::parse_options(&args(&["--launcher", "d"])).is_err());
    let missing = std::env::temp_dir().join("no-such-base-pack.dat");
    assert!(pyreal::check_base_pack(&missing).is_err());
}

/// The client and the plugin author's tool carry one version, which names the
/// release's tag.
#[test]
fn the_crates_carry_one_version_which_names_the_tag() {
    let ws = workspace_root();
    let v = Product::Pyreal.version(&ws).expect("the crates agree");
    version::parse_release_version(&v).expect("a release version");
    assert_eq!(Product::Pyreal.tag_for(&v), format!("v{v}"));
    assert_eq!(version::pyreal_manifests(&ws).len(), 2);
}

/// Every file the package reads from the tree is there.
#[test]
fn every_file_the_package_reads_from_the_tree_is_there() {
    let ws = workspace_root();
    for path in [
        "pyreal/about.toml",
        "pyreal/about.hbs",
        "LICENSE",
        "tools/plugin/Cargo.toml",
        "pyreal/client/Cargo.toml",
        "pyreal/client/assets/Pyreal.icon",
        "pyreal/client/assets/Pyreal.icns",
    ] {
        assert!(ws.join(path).exists(), "{path}");
    }
}

/// Pyreal's Windows programs are windowed, carry their icon and link the C runtime in; a console
/// program, one without its icon, or one importing the Visual C++ runtime is refused.
#[test]
fn a_windows_program_is_windowed_with_its_icon_and_a_static_runtime() {
    let w = target(WINDOWS);
    let good = pe(0x8664, 2, &["KERNEL32.dll", "USER32.dll"], &[], true);
    let d = headers::check_app_binary(&good, w, targets::PYREAL_GLIBC_FLOOR).expect("passes");
    assert!(d.contains("windowed"), "{d}");
    let console = pe(0x8664, 3, &["KERNEL32.dll"], &[], true);
    let e =
        headers::check_app_binary(&console, w, targets::PYREAL_GLIBC_FLOOR).expect_err("console");
    assert!(e.iter().any(|p| p.contains("windowed")), "{e:?}");
    let bare = pe(0x8664, 2, &["KERNEL32.dll"], &[], false);
    assert!(headers::check_app_binary(&bare, w, targets::PYREAL_GLIBC_FLOOR).is_err());
    let runtime = pe(0x8664, 2, &["VCRUNTIME140.dll"], &[], true);
    assert!(headers::check_app_binary(&runtime, w, targets::PYREAL_GLIBC_FLOOR).is_err());
}

/// Each Pyreal target builds on its own operating system only.
#[test]
fn dereth_builds_natively_and_names_what_is_missing() {
    let host = |os| targets::Host {
        os: Some(os),
        installed: None,
        glibc: None,
    };
    assert!(targets::plan_pyreal(target(WINDOWS), &host(Os::Windows)).is_ok());
    assert!(targets::plan_pyreal(target(LINUX), &host(Os::Windows)).is_err());
    assert!(targets::plan_pyreal(target(MAC_ARM), &host(Os::Mac)).is_ok());
    let missing = targets::Host {
        installed: Some(vec![WINDOWS.to_owned()]),
        ..host(Os::Mac)
    };
    let e = targets::plan_pyreal(target(MAC_ARM), &missing).expect_err("not installed");
    assert!(e.contains("rustup target add"), "{e}");
    assert!(targets::find_pyreal("aarch64-unknown-linux-gnu").is_err());
    assert!(
        targets::find_pyreal("x86_64-apple-darwin").is_err(),
        "Intel Macs are no longer released for"
    );
}

/// A Linux host whose glibc is newer than the release's floor is refused while the packages are
/// planned, before anything is built, and told where to build instead.
#[test]
fn a_linux_host_with_a_newer_glibc_is_refused_before_anything_is_built() {
    let host = |glibc| targets::Host {
        os: Some(Os::Linux),
        installed: None,
        glibc,
    };
    type Plan = fn(Target, &targets::Host) -> Result<(), String>;
    let plans: [Plan; 1] = [targets::plan_pyreal];
    for plan in plans {
        let e = plan(target(LINUX), &host(Some((2, 39)))).expect_err("too new");
        assert!(
            e.contains("glibc 2.39") && e.contains("2.35 floor") && e.contains("`ubuntu:22.04`"),
            "{e}"
        );
        assert!(plan(target(LINUX), &host(Some((2, 35)))).is_ok());
        assert!(plan(target(LINUX), &host(Some((2, 31)))).is_ok());
        assert!(
            plan(target(LINUX), &host(None)).is_ok(),
            "the header check still holds it"
        );
    }
    assert_eq!(targets::glibc_of("glibc 2.39\n"), Some((2, 39)));
    assert_eq!(targets::glibc_of("glibc 2.35"), Some((2, 35)));
    assert_eq!(targets::glibc_of("musl libc"), None);
}

/// The plugin author's tool is a release file of its own for each platform Pyreal ships, named for
/// the version and the platform, and the folder's classification reads it and its manifest back.
#[test]
fn the_plugin_authors_tool_is_a_release_file_for_each_platform() {
    let names: Vec<String> = targets::PYREAL_TRIPLES
        .iter()
        .map(|t| pyreal::tool_file_name(target(t), V))
        .collect();
    assert_eq!(
        names,
        [
            "pyreal-plugin-0.2.0-windows-x86_64.zip",
            "pyreal-plugin-0.2.0-linux-x86_64.tar.gz",
            "pyreal-plugin-0.2.0-macos-aarch64.tar.gz",
        ]
    );
    for triple in targets::PYREAL_TRIPLES {
        let t = target(triple);
        let name = pyreal::tool_file_name(t, V);
        assert_eq!(pyreal::classify(&name, V), Ok(Asset::Tool(t)));
        assert_eq!(
            pyreal::classify(&format!("{name}.manifest"), V),
            Ok(Asset::ManifestPart)
        );
        assert_eq!(guard::name_finding(&name), None, "{name}");
    }
    assert!(pyreal::classify("pyreal-plugin-0.1.0-windows-x86_64.zip", V).is_err());
    assert!(pyreal::classify("pyreal-plugin-0.2.0-windows-x86_64.tar.gz", V).is_err());
}

/// The tool's archive holds the program and its notices under one folder, the program executable
/// where the system has the bit; the deny scan refuses one missing a notice or holding anything
/// more.
#[test]
fn the_tools_archive_holds_the_program_and_its_notices_and_nothing_else() {
    let dir = std::env::temp_dir().join(format!("xtask-tool-archive-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");
    for triple in [WINDOWS, LINUX, MAC_ARM] {
        let t = target(triple);
        let members =
            pyreal::tool_members(t, b"program".to_vec(), "MIT", "the notice", "<html></html>");
        assert_eq!(
            members
                .iter()
                .map(|m| m.name.clone())
                .collect::<Vec<_>>()
                .len(),
            pyreal::tool_contents(t).len()
        );
        assert!(members[0].executable && members[0].name.starts_with("pyreal-plugin"));
        let write = |members: &[Member], name: &str| {
            let path = dir.join(name);
            let bytes = pyreal::tool_archive(t, V, members, 1_700_000_000).expect("archived");
            std::fs::write(&path, bytes).expect("written");
            path
        };
        let path = write(&members, &pyreal::tool_file_name(t, V));
        pyreal::check_tool_archive(&path, t, V).expect("passes the scan");
        let entries = archive::archive_entries(&path, &pyreal::tool_root(V)).expect("reads");
        assert!(pyreal::tool_findings(&entries, t).is_empty());

        let short: Vec<Member> = members
            .iter()
            .filter(|m| m.name != "NOTICE.txt")
            .cloned()
            .collect();
        let path = write(&short, &pyreal::tool_file_name(t, V));
        let refused = pyreal::check_tool_archive(&path, t, V).expect_err("a notice missing");
        assert!(refused.contains("NOTICE.txt"), "{refused}");
        let mut stray = members.clone();
        stray.push(Member {
            name: "client_portal.dat".into(),
            bytes: b"x".to_vec(),
            executable: false,
        });
        let path = write(&stray, &pyreal::tool_file_name(t, V));
        assert!(pyreal::check_tool_archive(&path, t, V).is_err());
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Notarizing takes all three parts of the API key or none, and waits for Apple's answer.
#[test]
fn notarizing_needs_the_whole_key_and_waits_for_the_answer() {
    use super::dmg::Notary;
    let s = |v: &str| Some(v.to_owned());
    assert_eq!(Notary::from_values(None, None, None), Ok(None));
    assert!(Notary::from_values(s("k.p8"), s("ID"), None).is_err());
    let n = Notary::from_values(s("k.p8"), s("ID"), s("ISS"))
        .unwrap()
        .unwrap();
    let args = n.submit_args("P.dmg");
    assert_eq!(args[..3], ["notarytool", "submit", "P.dmg"]);
    assert!(args.windows(2).any(|w| w == ["--key", "k.p8"]));
    assert!(args.windows(2).any(|w| w == ["--key-id", "ID"]));
    assert!(args.windows(2).any(|w| w == ["--issuer", "ISS"]));
    assert!(args.contains(&"--wait".to_owned()));
    assert_eq!(
        super::dmg::submission(r#"{"id":"abc","status":"Accepted","message":"x"}"#),
        Ok(("abc".to_owned(), "Accepted".to_owned()))
    );
    assert!(super::dmg::submission("not json").is_err());
}

/// The index checks a disk image against the size and SHA-256 its manifest part records.
#[test]
fn a_disk_image_is_checked_against_its_manifest_line() {
    let part = "aarch64-apple-darwin\tP.dmg\t12\tabcd\tdisk image\n\
                aarch64-apple-darwin\tP.dmg/Contents/MacOS/Pyreal\t5\tef01\tMach-O\n";
    assert!(super::dmg::recorded(part, "P.dmg", 12, "abcd"));
    assert!(!super::dmg::recorded(part, "P.dmg", 13, "abcd"));
    assert!(!super::dmg::recorded(part, "P.dmg", 12, "ffff"));
    assert!(!super::dmg::recorded(part, "Q.dmg", 12, "abcd"));
}
