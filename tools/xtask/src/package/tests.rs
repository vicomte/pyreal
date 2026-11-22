use super::archive::{self, Member};
use super::guard::{self, Entry, Kind};
use super::headers;
use super::notice;
use super::targets::{self, Arch, Target};
use super::version;
use super::*;
use crate::util::workspace_root;

fn target(triple: &str) -> Target {
    targets::find(triple).expect("a release target")
}

const WINDOWS: &str = "x86_64-pc-windows-msvc";
const LINUX_X64: &str = "x86_64-unknown-linux-gnu";
const MAC_ARM: &str = "aarch64-apple-darwin";

// ---------------------------------------------------------------- the deny scan

/// A flat package's files: the deny scan's fixture.
fn allowlist() -> Vec<String> {
    ["LICENSE", "NOTICE.txt", "README.md", "pyreal"]
        .map(str::to_owned)
        .to_vec()
}

/// The deny scan with every file of `allowlist` required, under the pyreal caps.
fn deny_scan(entries: &[Entry], allowlist: &[String]) -> Vec<String> {
    guard::scan_tree(
        entries,
        allowlist,
        &[],
        pyreal::FILE_CAP,
        pyreal::FILE_CAP * 4,
    )
}

fn clean_entries() -> Vec<Entry> {
    let mut e = vec![Entry {
        path: String::new(),
        kind: Kind::Dir,
        size: 0,
        head: Vec::new(),
    }];
    e.extend(allowlist().iter().map(|n| Entry::file(n, b"contents")));
    e
}

/// The complete allowlist of plain files passes, and so the scan is not refusing everything.
#[test]
fn the_complete_allowlist_of_plain_files_passes_the_scan() {
    assert_eq!(
        deny_scan(&clean_entries(), &allowlist()),
        Vec::<String>::new()
    );
}

/// A file carrying the game's data-container header is refused whatever it is called, even on the
/// allowlist.
#[test]
fn a_file_with_the_data_header_is_refused_whatever_its_name() {
    let mut bytes = vec![0u8; 0x400];
    bytes[0x140..0x144].copy_from_slice(&0x5442u32.to_le_bytes());
    let mut entries = clean_entries();
    let readme = entries
        .iter_mut()
        .find(|e| e.path == "README.md")
        .expect("on the allowlist");
    *readme = Entry::file("README.md", &bytes);
    let findings = deny_scan(&entries, &allowlist());
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].contains("data-file header"), "{findings:?}");
    // The same magic one byte away is not the header.
    let mut shifted = vec![0u8; 0x400];
    shifted[0x141..0x145].copy_from_slice(&0x5442u32.to_le_bytes());
    assert!(!guard::has_dat_magic(&shifted));
    assert!(guard::has_dat_magic(&bytes));
}

/// The game's files, a world pack, the databases and captures are refused by name, in any case;
/// the base pack passes by its own name alone.
#[test]
fn data_state_and_capture_names_are_refused_in_any_case() {
    for name in [
        "client_portal.dat",
        "CLIENT_CELL_1.DAT",
        "Client_HighRes.dat",
        "something.DAT",
        "ENHANCED-ENVIRONMENT.DAT",
        "acclient.exe",
        "AcClient.map",
        "00.00.11.6096.what",
        "world.pack",
        "world.pack.report.json",
        "shard.db",
        "auth.db",
        "auth.db-wal",
        "overlay.sqlite",
        "session.pcap",
        "fixtures/anything.txt",
        "packet-captures/raw/a.bin",
        "x/scrub-map/names.json",
    ] {
        assert!(guard::name_finding(name).is_some(), "{name} passed");
    }
    for name in [
        "README.md",
        "pyreal.exe",
        "database.md",
        "Contents/Resources/base-pack.dat",
    ] {
        assert_eq!(guard::name_finding(name), None, "{name} refused");
    }
}

/// A link, a folder, an extra file, a missing file and an oversized file are each refused.
#[test]
fn links_folders_strays_gaps_and_oversized_files_are_each_refused() {
    let allow = allowlist();
    let with = |extra: Entry| {
        let mut e = clean_entries();
        e.push(extra);
        deny_scan(&e, &allow)
    };
    let link = with(Entry {
        path: "dats".to_owned(),
        kind: Kind::Link("a symbolic link".to_owned()),
        size: 0,
        head: Vec::new(),
    });
    assert!(link.iter().any(|f| f.contains("no links")), "{link:?}");
    let dir = with(Entry {
        path: "docs".to_owned(),
        kind: Kind::Dir,
        size: 0,
        head: Vec::new(),
    });
    assert!(dir.iter().any(|f| f.contains("a folder")), "{dir:?}");
    let stray = with(Entry::file("notes.txt", b"x"));
    assert!(
        stray.iter().any(|f| f.contains("not on the allowlist")),
        "{stray:?}"
    );

    let mut missing = clean_entries();
    missing.retain(|e| e.path != "NOTICE.txt");
    let gap = deny_scan(&missing, &allow);
    assert!(
        gap.iter()
            .any(|f| f.contains("NOTICE.txt") && f.contains("missing")),
        "{gap:?}"
    );

    let mut big = clean_entries();
    big.iter_mut()
        .find(|e| e.path == "pyreal")
        .expect("listed")
        .size = pyreal::FILE_CAP + 1;
    let over = deny_scan(&big, &allow);
    assert!(
        over.iter().any(|f| f.contains("cap for one file")),
        "{over:?}"
    );
    let total = guard::scan_tree(&clean_entries(), &allow, &[], pyreal::FILE_CAP, 10);
    assert!(
        total.iter().any(|f| f.contains("cap for one package")),
        "{total:?}"
    );
}

/// The leading bytes of a container with its header at `at` and the data subset `subset`.
fn container_head(at: usize, subset: &[u8; 4]) -> Vec<u8> {
    let mut b = vec![0u8; guard::HEAD_LEN];
    b[at..at + 4].copy_from_slice(&0x5442u32.to_le_bytes());
    b[at + 0x0C..at + 0x10].copy_from_slice(&1u32.to_le_bytes());
    b[at + 0x10..at + 0x14].copy_from_slice(subset);
    b
}

/// A file of `size` bytes whose leading bytes are `head`.
fn sized(path: &str, head: &[u8], size: u64) -> Entry {
    Entry {
        size,
        ..Entry::file(path, head)
    }
}

/// The flat package's files with the base pack allowed beside them.
fn scan_with_base(extra: Entry) -> Vec<String> {
    let mut e = clean_entries();
    e.push(extra);
    guard::scan_tree(
        &e,
        &allowlist(),
        &[guard::BASE_PACK.to_owned()],
        pyreal::FILE_CAP,
        pyreal::TOTAL_CAP,
    )
}

/// The base pack has no size cap: far over the file and package caps it passes, and it does not
/// count toward the package's total.
#[test]
fn the_base_pack_passes_at_any_size() {
    let pack = container_head(0x140, b"Pack");
    for size in [1, pyreal::FILE_CAP + 1, pyreal::TOTAL_CAP * 4, 1 << 31] {
        let found = scan_with_base(sized(guard::BASE_PACK, &pack, size));
        assert_eq!(found, Vec::<String>::new(), "{size} bytes");
    }
}

/// Retail files of every era are refused under the old file cap: by name, by their header, and as
/// the base pack by name when they are not a packed art pack.
#[test]
fn retail_files_under_the_old_cap_are_refused_by_name_or_header() {
    let modern = container_head(0x140, &[0; 4]);
    let hires = container_head(0x140, b"HiFi");
    let classic = container_head(0x12C, &[0; 4]);
    // 1999's portal.dat, 2005's portal.dat and EoR's client_highres.dat, at their sizes.
    for (name, head, size) in [
        ("portal.dat", &classic, 95u64 << 20),
        ("portal.dat", &classic, 272 << 20),
        ("client_highres.dat", &hires, 127 << 20),
    ] {
        let found = scan_with_base(sized(name, head, size));
        assert!(found.iter().any(|f| f.contains("a .dat file")), "{found:?}");
        let renamed = scan_with_base(sized("pyreal-data", head, size));
        assert!(
            renamed.iter().any(|f| f.contains("data-file header")),
            "{renamed:?}"
        );
        let as_base = scan_with_base(sized(guard::BASE_PACK, head, size));
        assert!(
            as_base.iter().any(|f| f.contains("not a packed art pack")),
            "{as_base:?}"
        );
    }
    for head in [&modern, &[0u8; 16].to_vec(), &Vec::new()] {
        let found = scan_with_base(sized(guard::BASE_PACK, head, 1 << 20));
        assert!(
            found.iter().any(|f| f.contains("not a packed art pack")),
            "{found:?}"
        );
    }
}

/// A packed art pack's header is the container's at `0x140` with the subset `Pack`, as the packer
/// writes it.
#[test]
fn the_packed_art_pack_header_is_the_packers() {
    let pack = container_head(0x140, b"Pack");
    assert!(guard::is_packed_art_pack(&pack));
    assert!(!guard::is_packed_art_pack(&container_head(0x12C, b"Pack")));
    assert!(!guard::is_packed_art_pack(&pack[..0x150]));
    // The container writer's own header, made as the art packer makes it.
    let made = |subset: u32| {
        let s = pyreal_dat::MemoryFile::default();
        {
            use pyreal_dat::DatStorageMut;
            s.clone().write_all_at(0x1_0000 - 1, &[0]).expect("sized");
        }
        pyreal_dat::DatWriter::create_in(
            PathBuf::from("pack.dat"),
            Box::new(s.clone()),
            0x400,
            pyreal_dat::container::PORTAL_DATFILE,
            subset,
            0x1_0000,
        )
        .expect("made");
        s.bytes()
    };
    assert!(guard::is_packed_art_pack(&made(u32::from_le_bytes(
        *b"Pack"
    ))));
    assert!(!guard::is_packed_art_pack(&made(0)));
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xtask-package-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temp directory");
    dir
}

/// The staging walk reports what is really on disk: files with their heads, and a folder inside.
#[test]
fn the_staging_walk_reports_nested_folders_and_file_heads() {
    let dir = temp("walk");
    std::fs::write(dir.join("README.md"), b"hello").expect("write");
    std::fs::create_dir(dir.join("inner")).expect("mkdir");
    std::fs::write(dir.join("inner").join("portal.dat"), b"x").expect("write");
    let entries = guard::staged_entries(&dir).expect("walk");
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["", "README.md", "inner", "inner/portal.dat"]);
    assert_eq!(entries[1].head, b"hello");
    let findings = deny_scan(&entries, &["README.md".to_owned()]);
    assert!(
        findings.iter().any(|f| f.starts_with("inner:")),
        "{findings:?}"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.starts_with("inner/portal.dat: a .dat")),
        "{findings:?}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// A symbolic link in the staging folder is reported as a link, not followed.
#[cfg(unix)]
#[test]
fn a_symbolic_link_in_the_staging_folder_is_reported_as_a_link() {
    let dir = temp("symlink");
    std::fs::write(dir.join("target.bin"), b"x").expect("write");
    std::os::unix::fs::symlink(dir.join("target.bin"), dir.join("README.md")).expect("link");
    let entries = guard::staged_entries(&dir).expect("walk");
    let readme = entries
        .iter()
        .find(|e| e.path == "README.md")
        .expect("listed");
    assert!(matches!(readme.kind, Kind::Link(_)), "{readme:?}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_build_machine_folder_in_a_file_is_found_with_either_slash() {
    let needles = vec![r"C:\Users\me".to_owned()];
    assert_eq!(
        guard::local_paths_in(b"at c:/users/me/.cargo/x.rs", &needles),
        needles
    );
    assert_eq!(
        guard::local_paths_in(br"at C:\Users\me\src", &needles),
        needles
    );
    assert!(guard::local_paths_in(b"/cargo/registry/x.rs", &needles).is_empty());
}

/// The folders looked for include the target folder, wherever it is: build scripts write the
/// source they generate there, and a program can carry its path.
#[test]
fn the_machine_folders_looked_for_include_the_workspace_and_the_target_folder() {
    let ws = workspace_root();
    let folders = guard::local_folders(&ws);
    assert!(folders.contains(&ws.display().to_string()), "{folders:?}");
    assert!(
        folders.contains(&crate::util::target_dir().display().to_string()),
        "{folders:?}"
    );
}

// ---------------------------------------------------------------- archives

fn members() -> Vec<Member> {
    vec![
        Member {
            name: "pyreal".to_owned(),
            bytes: b"\x7fELF binary".to_vec(),
            executable: true,
        },
        Member {
            name: "LICENSE".to_owned(),
            bytes: b"licence text".to_vec(),
            executable: false,
        },
    ]
}

/// The same files and commit give byte-identical archives, the gzip header carries no time, and
/// every entry is under the one top-level folder in name order.
#[test]
fn archives_are_byte_identical_for_the_same_files_and_commit() {
    let epoch = 1_700_000_000;
    let a = archive::tar_gz_bytes("root", &members(), epoch).expect("tar.gz");
    let b = archive::tar_gz_bytes("root", &members(), epoch).expect("tar.gz");
    assert_eq!(a, b);
    assert_eq!(&a[4..8], &[0, 0, 0, 0], "the gzip header's time is zero");
    let z1 = archive::zip_bytes("root", &members(), epoch).expect("zip");
    let z2 = archive::zip_bytes("root", &members(), epoch).expect("zip");
    assert_eq!(z1, z2);
    for entries in [
        archive::tar_gz_entries(&a).expect("read"),
        archive::zip_entries(&z1).expect("read"),
    ] {
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["root/", "root/LICENSE", "root/pyreal"]);
        assert_eq!(entries[2].head, b"\x7fELF binary");
    }
    // A different commit time is a different archive.
    assert_ne!(
        a,
        archive::tar_gz_bytes("root", &members(), epoch + 1).expect("tar.gz")
    );
}

/// A tar entry carries the commit's time, mode 0755 for a binary and 0644 otherwise, and no owner.
#[test]
fn tar_entries_carry_the_commit_time_fixed_modes_and_no_owner() {
    let epoch = 1_700_000_000i64;
    let tar = archive::tar_bytes("root", &members(), epoch).expect("tar");
    let header = |i: usize| &tar[i * 512..i * 512 + 512];
    let text = |h: &[u8], r: std::ops::Range<usize>| {
        String::from_utf8_lossy(&h[r])
            .trim_end_matches('\0')
            .to_owned()
    };
    // Entry 0 the folder, 1 LICENSE (one data block follows), then the binary.
    let license = header(1);
    let binary = header(3);
    assert_eq!(text(license, 0..100), "root/LICENSE");
    assert_eq!(text(license, 100..108), "0000644");
    assert_eq!(text(binary, 100..108), "0000755");
    assert_eq!(text(binary, 108..116), "0000000");
    assert_eq!(text(binary, 265..297), "", "no owner name");
    assert_eq!(
        u64::from_str_radix(&text(binary, 136..148), 8).expect("octal"),
        1_700_000_000
    );
}

/// The archive scan reads what was written: a stray member, a symbolic link and a path outside the
/// top-level folder are refused from the archive itself.
#[test]
fn the_archive_scan_refuses_strays_links_and_escapes_in_the_written_archive() {
    let root = "pyreal-0.1.0";
    let good: Vec<Member> = allowlist()
        .into_iter()
        .map(|name| Member {
            name,
            bytes: b"x".to_vec(),
            executable: false,
        })
        .collect();
    let scan = |bytes: &[u8], zip: bool| {
        let raw = if zip {
            archive::zip_entries(bytes).expect("read")
        } else {
            archive::tar_gz_entries(bytes).expect("read")
        };
        deny_scan(&archive::relative_to_root(raw, root), &allowlist())
    };
    let clean = archive::tar_gz_bytes(root, &good, 0).expect("tar.gz");
    assert_eq!(scan(&clean, false), Vec::<String>::new());
    let clean_zip = archive::zip_bytes(root, &good, 0).expect("zip");
    assert_eq!(scan(&clean_zip, true), Vec::<String>::new());

    let mut stray = good.clone();
    stray.push(Member {
        name: "client_portal.dat".to_owned(),
        bytes: b"x".to_vec(),
        executable: false,
    });
    let found = scan(&archive::zip_bytes(root, &stray, 0).expect("zip"), true);
    assert!(
        found
            .iter()
            .any(|f| f.starts_with("client_portal.dat: a .dat")),
        "{found:?}"
    );

    // A tar with a symbolic link entry: the clean tar with one more header of type `2`.
    let mut tar = archive::tar_bytes(root, &good, 0).expect("tar");
    tar.truncate(tar.len() - 1024);
    let mut link = [0u8; 512];
    let name = format!("{root}/dats");
    link[..name.len()].copy_from_slice(name.as_bytes());
    link[124..135].copy_from_slice(b"00000000000");
    link[156] = b'2';
    tar.extend_from_slice(&link);
    tar.resize(tar.len() + 1024, 0);
    let entries = archive::relative_to_root(archive::tar_entries(&tar).expect("read"), root);
    let found = deny_scan(&entries, &allowlist());
    assert!(
        found.iter().any(|f| f.starts_with("dats: a symbolic link")),
        "{found:?}"
    );

    let outside = archive::tar_gz_bytes("elsewhere", &good, 0).expect("tar.gz");
    let found = scan(&outside, false);
    assert!(
        found.iter().any(|f| f.starts_with("../elsewhere")),
        "{found:?}"
    );
}

// ---------------------------------------------------------------- versions, targets and hosts

/// A release version is three plain numbers.
#[test]
fn a_release_version_is_three_plain_numbers() {
    assert_eq!(version::parse_release_version("0.1.0"), Ok([0, 1, 0]));
    assert_eq!(version::parse_release_version("10.20.30"), Ok([10, 20, 30]));
    assert_eq!(version::parse_release_version("0.1.0-rc.1"), Ok([0, 1, 0]));
    assert!(version::is_prerelease("0.1.0-rc.1"));
    assert!(!version::is_prerelease("0.1.0"));
    for bad in [
        "0.1",
        "0.1.0.0",
        "0.1.0+b",
        "0.1.0-",
        "0.1.0-rc..1",
        "0.1.0-rc_1",
        "01.2.3",
        "v0.1.0",
        "0..1",
        "",
    ] {
        assert!(
            version::parse_release_version(bad).is_err(),
            "{bad} accepted"
        );
    }
}

/// A target the releases do not name is refused, naming the ones they do.
#[test]
fn an_unknown_target_is_refused_naming_the_known_ones() {
    let e = targets::find("riscv64gc-unknown-linux-gnu").expect_err("not a target");
    assert!(e.contains(WINDOWS), "{e}");
    assert_eq!(target(LINUX_X64).exe_suffix(), "");
    assert_eq!(target(WINDOWS).exe_suffix(), ".exe");
}

/// The commit time is written as ISO 8601 in UTC.
#[test]
fn the_commit_time_is_written_in_utc() {
    let facts = BuildFacts {
        commit: "a".repeat(40),
        hash: "a".repeat(8),
        epoch: 1_700_000_000,
        source_url: String::new(),
        update_url: String::new(),
    };
    assert_eq!(facts.iso_time(), "2023-11-14T22:13:20Z");
}

/// A release build renames the target folder as well as the checkout, after it, so a target
/// folder inside the checkout takes its own name and one outside it is not left out.
#[test]
fn a_release_build_renames_the_target_folder_after_the_checkout() {
    let ws = workspace_root();
    let remaps = local_remaps(&ws);
    let at = |name: &str| remaps.iter().position(|(_, to)| *to == name);
    assert_eq!(at("/pyreal"), Some(0), "{remaps:?}");
    let target = crate::util::target_dir();
    if target != ws {
        let i = at("/target").expect("the target folder is renamed");
        assert_eq!(remaps[i].0, target);
    }
}

/// The calendar conversion is right at the epoch, across a leap day and before 1970.
#[test]
fn the_calendar_conversion_is_right_at_the_edges() {
    assert_eq!(archive::civil(0), (1970, 1, 1, 0, 0, 0));
    assert_eq!(archive::civil(951_782_400), (2000, 2, 29, 0, 0, 0));
    assert_eq!(archive::civil(951_868_799), (2000, 2, 29, 23, 59, 59));
    assert_eq!(archive::civil(951_868_800), (2000, 3, 1, 0, 0, 0));
    assert_eq!(archive::civil(-1), (1969, 12, 31, 23, 59, 59));
}

// ---------------------------------------------------------------- header checks

/// A PE32+ image with one section holding the import and delay-import tables.
pub(super) fn pe(
    machine: u16,
    subsystem: u16,
    imports: &[&str],
    delayed: &[&str],
    resources: bool,
) -> Vec<u8> {
    let mut b = vec![0u8; 0x800];
    let put16 =
        |b: &mut Vec<u8>, at: usize, v: u16| b[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let put32 =
        |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    b[..2].copy_from_slice(b"MZ");
    put32(&mut b, 0x3C, 0x80);
    b[0x80..0x84].copy_from_slice(b"PE\0\0");
    put16(&mut b, 0x84, machine);
    put16(&mut b, 0x86, 1);
    put16(&mut b, 0x94, 240);
    let opt = 0x98;
    put16(&mut b, opt, 0x20B);
    put16(&mut b, opt + 68, subsystem);
    put32(&mut b, opt + 108, 16);
    let (va, raw) = (0x1000u32, 0x200usize);
    let delay_off = (imports.len() + 1) * 20;
    let mut name_off = delay_off + (delayed.len() + 1) * 32;
    let mut name_rva = |b: &mut Vec<u8>, name: &str| {
        let at = raw + name_off;
        b[at..at + name.len()].copy_from_slice(name.as_bytes());
        let rva = va + u32::try_from(name_off).expect("small");
        name_off += name.len() + 1;
        rva
    };
    for (i, name) in imports.iter().enumerate() {
        let rva = name_rva(&mut b, name);
        put32(&mut b, raw + i * 20, 1);
        put32(&mut b, raw + i * 20 + 12, rva);
    }
    for (i, name) in delayed.iter().enumerate() {
        let rva = name_rva(&mut b, name);
        put32(&mut b, raw + delay_off + i * 32, 1);
        put32(&mut b, raw + delay_off + i * 32 + 4, rva);
    }
    put32(&mut b, opt + 112 + 8, va);
    put32(&mut b, opt + 112 + 12, 20);
    if resources {
        put32(&mut b, opt + 112 + 16, va + 0x500);
        put32(&mut b, opt + 112 + 20, 0x10);
    }
    if !delayed.is_empty() {
        put32(
            &mut b,
            opt + 112 + 13 * 8,
            va + u32::try_from(delay_off).expect("small"),
        );
        put32(&mut b, opt + 112 + 13 * 8 + 4, 32);
    }
    let s = opt + 240;
    b[s..s + 6].copy_from_slice(b".idata");
    put32(&mut b, s + 8, 0x600);
    put32(&mut b, s + 12, va);
    put32(&mut b, s + 16, 0x600);
    put32(&mut b, s + 20, u32::try_from(raw).expect("small"));
    b
}

/// A statically linked x86-64 console program passes the shared PE check, and the parse reads the
/// imports it was given.
#[test]
fn a_static_console_x64_pe_passes_and_its_imports_are_read() {
    let bytes = pe(
        0x8664,
        3,
        &["KERNEL32.dll", "WS2_32.dll"],
        &["bcrypt.dll"],
        false,
    );
    let parsed = headers::parse_pe(&bytes).expect("parses");
    assert_eq!(parsed.imports, ["KERNEL32.dll", "WS2_32.dll", "bcrypt.dll"]);
    assert_eq!(pe_problems(&bytes), Vec::<String>::new());
}

/// What the shared PE check finds in a Windows program.
fn pe_problems(bytes: &[u8]) -> Vec<String> {
    headers::check_pe(&headers::parse_pe(bytes).expect("parses"), target(WINDOWS))
}

/// A Mac program checked as the client is.
fn mac(bytes: &[u8]) -> Result<String, Vec<String>> {
    headers::check_app_binary(bytes, target(MAC_ARM), targets::PYREAL_GLIBC_FLOOR)
}

/// The Visual C++ runtime, imported or delay-loaded, fails the check and names the fix.
#[test]
fn a_pe_importing_the_visual_cpp_runtime_is_refused() {
    for (imports, delayed) in [
        (&["KERNEL32.dll", "VCRUNTIME140.dll"][..], &[][..]),
        (
            &["KERNEL32.dll"][..],
            &["api-ms-win-crt-runtime-l1-1-0.dll"][..],
        ),
        (&["MSVCP140.dll"][..], &[][..]),
    ] {
        let bytes = pe(0x8664, 3, imports, delayed, true);
        let problems = pe_problems(&bytes);
        assert!(
            problems.iter().any(|p| p.contains("crt-static")),
            "{problems:?}"
        );
    }
}

/// The wrong machine and a GUI subsystem are each refused by the shared PE check.
#[test]
fn a_console_pe_with_the_wrong_machine_or_subsystem_is_refused() {
    let arm = pe(0xAA64, 3, &["KERNEL32.dll"], &[], true);
    assert!(!pe_problems(&arm).is_empty());
    let gui = pe(0x8664, 2, &["KERNEL32.dll"], &[], true);
    let problems = pe_problems(&gui);
    assert!(
        problems.iter().any(|p| p.contains("console")),
        "{problems:?}"
    );
    assert!(headers::parse_pe(b"not an executable").is_err());
}

/// This test binary, a real PE on Windows, parses as x86-64 importing KERNEL32.
#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn a_real_windows_binary_parses() {
    let bytes = std::fs::read(std::env::current_exe().expect("exe")).expect("read");
    let parsed = headers::parse_pe(&bytes).expect("parses");
    assert_eq!(parsed.machine, headers::IMAGE_FILE_MACHINE_AMD64);
    assert!(parsed.pe32_plus);
    assert!(
        parsed
            .imports
            .iter()
            .any(|i| i.eq_ignore_ascii_case("kernel32.dll")),
        "{:?}",
        parsed.imports
    );
}

/// A 64-bit little-endian ELF: one load segment mapping the file, the loader, and a dynamic
/// section with `DT_NEEDED` entries and one version-needs record.
pub(super) fn elf(machine: u16, interp: &str, needed: &[&str], glibc: &[&str]) -> Vec<u8> {
    let mut b = vec![0u8; 0x800];
    let p16 = |b: &mut Vec<u8>, at: usize, v: u16| b[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let p32 = |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    let p64 = |b: &mut Vec<u8>, at: usize, v: u64| b[at..at + 8].copy_from_slice(&v.to_le_bytes());
    b[..4].copy_from_slice(b"\x7fELF");
    b[4] = 2;
    b[5] = 1;
    b[6] = 1;
    p16(&mut b, 16, 3);
    p16(&mut b, 18, machine);
    p64(&mut b, 32, 64);
    p16(&mut b, 54, 56);
    p16(&mut b, 56, 3);
    let (interp_at, strtab, verneed, dynamic) = (0x100usize, 0x200usize, 0x300usize, 0x400usize);
    b[interp_at..interp_at + interp.len()].copy_from_slice(interp.as_bytes());
    let mut str_end = 1usize;
    let mut add = |b: &mut Vec<u8>, s: &str| {
        let off = str_end;
        b[strtab + off..strtab + off + s.len()].copy_from_slice(s.as_bytes());
        str_end += s.len() + 1;
        u64::try_from(off).expect("small")
    };
    let needed_offs: Vec<u64> = needed.iter().map(|n| add(&mut b, n)).collect();
    let file = add(&mut b, "libc.so.6");
    let glibc_offs: Vec<u64> = glibc.iter().map(|g| add(&mut b, g)).collect();
    // Program headers: LOAD (the whole file), INTERP, DYNAMIC.
    let ph = |b: &mut Vec<u8>, i: usize, kind: u32, off: u64, size: u64| {
        let at = 64 + i * 56;
        p32(b, at, kind);
        p64(b, at + 8, off);
        p64(b, at + 16, off);
        p64(b, at + 32, size);
        p64(b, at + 40, size);
    };
    let mut entries: Vec<(u64, u64)> = needed_offs.iter().map(|&o| (1, o)).collect();
    entries.push((5, strtab as u64));
    entries.push((10, str_end as u64));
    if !glibc.is_empty() {
        entries.push((0x6fff_fffe, verneed as u64));
        entries.push((0x6fff_ffff, 1));
    }
    entries.push((0, 0));
    ph(&mut b, 0, 1, 0, 0x800);
    ph(&mut b, 1, 3, interp_at as u64, interp.len() as u64 + 1);
    ph(&mut b, 2, 2, dynamic as u64, entries.len() as u64 * 16);
    for (i, (tag, val)) in entries.iter().enumerate() {
        p64(&mut b, dynamic + i * 16, *tag);
        p64(&mut b, dynamic + i * 16 + 8, *val);
    }
    p16(&mut b, verneed, 1);
    p16(
        &mut b,
        verneed + 2,
        u16::try_from(glibc.len()).expect("small"),
    );
    p32(&mut b, verneed + 4, u32::try_from(file).expect("small"));
    p32(&mut b, verneed + 8, 16);
    for (i, off) in glibc_offs.iter().enumerate() {
        let aux = verneed + 16 + i * 16;
        p32(&mut b, aux + 8, u32::try_from(*off).expect("small"));
        p32(&mut b, aux + 12, if i + 1 < glibc.len() { 16 } else { 0 });
    }
    b
}

const X64_LOADER: &str = "/lib64/ld-linux-x86-64.so.2";

/// The ELF parse reads a binary's needs and glibc versions, and the glibc names parse.
#[test]
fn an_elf_s_needs_and_glibc_versions_are_read() {
    let bytes = elf(
        62,
        X64_LOADER,
        &["libc.so.6", "libm.so.6", "libgcc_s.so.1"],
        &["GLIBC_2.2.5", "GLIBC_2.28", "GLIBC_2.14"],
    );
    let parsed = headers::parse_elf(&bytes).expect("parses");
    assert_eq!(parsed.needed, ["libc.so.6", "libm.so.6", "libgcc_s.so.1"]);
    assert_eq!(parsed.glibc.iter().max(), Some(&(2, 28, 0)));
    assert_eq!(headers::parse_glibc("GLIBC_2.2.5"), Some((2, 2, 5)));
    assert_eq!(headers::parse_glibc("GLIBC_PRIVATE"), None);
    assert_eq!(
        headers::linux_interp(Arch::Aarch64),
        "/lib/ld-linux-aarch64.so.1"
    );
}

/// A 64-bit Mach-O with the given libraries, build version and signature.
pub(super) fn macho(
    cputype: u32,
    dylibs: &[&str],
    min: Option<(u32, u32)>,
    signed: bool,
) -> Vec<u8> {
    let mut cmds: Vec<u8> = Vec::new();
    let mut n = 0u32;
    let u32le = |v: u32| v.to_le_bytes();
    for d in dylibs {
        let mut name = d.as_bytes().to_vec();
        name.push(0);
        name.resize(name.len().div_ceil(8) * 8, 0);
        let size = 24 + u32::try_from(name.len()).expect("small");
        for v in [0xC, size, 24, 0, 0, 0] {
            cmds.extend_from_slice(&u32le(v));
        }
        cmds.extend_from_slice(&name);
        n += 1;
    }
    if let Some((major, minor)) = min {
        for v in [0x32, 24, 1, (major << 16) | (minor << 8), 0, 0] {
            cmds.extend_from_slice(&u32le(v));
        }
        n += 1;
    }
    if signed {
        for v in [0x1D, 16, 0, 0] {
            cmds.extend_from_slice(&u32le(v));
        }
        n += 1;
    }
    let mut b = Vec::new();
    for v in [
        0xFEED_FACF,
        cputype,
        0,
        2,
        n,
        u32::try_from(cmds.len()).expect("small"),
        0,
        0,
    ] {
        b.extend_from_slice(&u32le(v));
    }
    b.extend_from_slice(&cmds);
    b
}

const SYSTEM: &[&str] = &[
    "/usr/lib/libSystem.B.dylib",
    "/System/Library/Frameworks/Security.framework/Versions/A/Security",
];

/// A signed arm64 binary linking only system libraries at macOS 11 passes.
#[test]
fn a_macho_linking_only_the_system_at_the_floor_passes() {
    let arm = macho(headers::CPU_TYPE_ARM64, SYSTEM, Some((11, 0)), true);
    let parsed = headers::parse_macho(&arm).expect("parses");
    assert_eq!(parsed.dylibs, SYSTEM);
    assert_eq!(parsed.min_os, Some((11, 0, 0)));
    assert!(mac(&arm).is_ok());
}

/// A Homebrew library, an unsigned arm64 binary, a minimum above the floor and the wrong CPU are
/// each refused.
#[test]
fn a_macho_with_foreign_libraries_no_signature_or_a_newer_minimum_is_refused() {
    let brew = macho(
        headers::CPU_TYPE_ARM64,
        &[
            "/usr/lib/libSystem.B.dylib",
            "/opt/homebrew/lib/libsqlite3.dylib",
        ],
        Some((11, 0)),
        true,
    );
    let p = mac(&brew).expect_err("homebrew");
    assert!(p.iter().any(|m| m.contains("/opt/homebrew")), "{p:?}");
    let unsigned = macho(headers::CPU_TYPE_ARM64, SYSTEM, Some((11, 0)), false);
    let p = mac(&unsigned).expect_err("unsigned");
    assert!(p.iter().any(|m| m.contains("signature")), "{p:?}");
    let newer = macho(headers::CPU_TYPE_ARM64, SYSTEM, Some((14, 0)), true);
    let p = mac(&newer).expect_err("14.0");
    assert!(p.iter().any(|m| m.contains("14.0")), "{p:?}");
    let intel = macho(headers::CPU_TYPE_X86_64, SYSTEM, Some((11, 0)), true);
    assert!(mac(&intel).is_err());
}

// ---------------------------------------------------------------- notices

/// The crate list comes from cargo tree once per crate, with path, proc-macro and repeat markers
/// dropped.
#[test]
fn the_crate_list_is_read_from_cargo_tree_once_per_crate() {
    let out = "pyreal-client v0.1.0 (C:\\w\\pyreal\\client)|MIT\n\
               log v0.4.22|MIT OR Apache-2.0\n\
               serde_derive v1.0.200 (proc-macro)|MIT OR Apache-2.0\n\
               log v0.4.22|MIT OR Apache-2.0 (*)\n\
               pyreal-dat v0.0.0 (C:\\w\\core\\dat)|MIT\n";
    let crates = notice::parse_cargo_tree(out);
    let names: Vec<(&str, &str)> = crates
        .iter()
        .map(|c| (c.name.as_str(), c.version.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("log", "0.4.22"),
            ("pyreal-client", "0.1.0"),
            ("pyreal-dat", "0.0.0"),
            ("serde_derive", "1.0.200")
        ]
    );
    assert_eq!(crates[0].licence, "MIT OR Apache-2.0");
    assert!(
        notice::is_own(&crates[1]) && notice::is_own(&crates[2]) && !notice::is_own(&crates[0])
    );
}

/// The per-binary licence pages join into one document: the first page's frame, then each
/// binary's section under its name.
#[test]
fn the_licence_pages_join_into_one_document_with_a_section_per_binary() {
    let page = |body: &str| {
        format!(
            "<html><head>h</head><body>{}{body}{}</body></html>",
            notice::SECTION_START,
            notice::SECTION_END
        )
    };
    let joined = notice::splice_licence_pages(&[
        ("pyreal-client", page("client licences")),
        ("engine-view", page("tool licences")),
    ])
    .expect("joined");
    assert!(joined.starts_with("<html><head>h</head><body>"), "{joined}");
    assert!(joined.ends_with("</body></html>"), "{joined}");
    let client = joined
        .find("Linked into pyreal-client")
        .expect("client section");
    let tool = joined
        .find("Linked into engine-view")
        .expect("tool section");
    assert!(client < joined.find("client licences").expect("body") && tool > client);
    assert!(joined.contains("tool licences"));
    assert!(notice::splice_licence_pages(&[("x", "<html>no markers</html>".to_owned())]).is_err());
}

/// The build hash is the one handed on when there is one, else git's, and only hex counts.
#[test]
fn the_build_hash_is_the_one_handed_on_else_gits() {
    use super::build_hash::pick;
    let git = || Some("8CA518E9\n".to_owned());
    assert_eq!(pick(Some("c15a72dd"), git).as_deref(), Some("c15a72dd"));
    assert_eq!(pick(None, git).as_deref(), Some("8ca518e9"));
    assert_eq!(
        pick(Some(""), git).as_deref(),
        Some("8ca518e9"),
        "empty is none"
    );
    assert_eq!(pick(Some("not hex"), || None), None);
    assert_eq!(pick(None, || None), None, "no git, no hash");
    assert_eq!(pick(None, || Some(String::new())), None);
}

/// `build.py` names its `dist/` folder by the build hash's own git rule and hands that same
/// value on, so the folder and the hash the app shows cannot differ.
#[test]
fn build_py_names_its_folder_by_the_build_hash_and_hands_it_on() {
    use super::build_hash::{ENV, GIT_ARGS};
    let src = std::fs::read_to_string(workspace_root().join("build.py")).expect("build.py");
    let args = GIT_ARGS.map(|a| format!("\"{a}\"")).join(", ");
    let rule = format!("commit = run([\"git\", {args}], env).strip()");
    assert!(src.contains(&rule), "build.py no longer runs `{rule}`");
    assert!(
        src.contains("out = cfg[\"out_dir\"] / commit"),
        "the folder is not the hash"
    );
    let handed = format!("env[\"{ENV}\"] = commit");
    assert!(
        src.contains(&handed),
        "build.py does not hand the hash on (`{handed}`)"
    );
}

/// The release build is handed the build hash with the commit.
#[test]
fn the_release_build_is_handed_the_build_hash() {
    let facts = build_facts(&workspace_root(), true).expect("the checkout's facts");
    assert!(
        !facts.hash.is_empty() && facts.commit.starts_with(&facts.hash),
        "{facts:?}"
    );
    let env = pyreal::build_env(&workspace_root(), &facts, target(LINUX_X64));
    assert!(
        env.contains(&(super::build_hash::ENV.to_owned(), facts.hash.clone())),
        "{env:?}"
    );
}
