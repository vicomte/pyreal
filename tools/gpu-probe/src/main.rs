//! Measures what the GPU layer can do on this machine: the features and limits the renderer's
//! design depends on, each proved by a small timed workload.
//!
//! **Depends on** no workspace crate: only `wgpu`, the same version as the client's.
//! **Used by** nothing; people run it.
//!
//! **Must never** be depended on by the client: it is a measuring tool, not a capability table.
//!
//! ```text
//! pyreal-gpu-probe [--json <path>]
//! ```
//!
//! It asks for the high-performance adapter (`WGPU_BACKEND` picks the API) and a device with
//! every feature and limit that adapter offers, then runs one section per question: bindless
//! textures, multi-draw indirect, ray queries, timestamps, limits and large buffers, immediates,
//! subgroups, texture formats, and the rest. Each workload checks its own result. The table goes
//! to the standard output, and `--json` writes the same rows, the timings and the adapter's full
//! feature list and limits.
//!
//! Run it with `--release`: a debug build turns on wgpu's validation layers, which skew the
//! timings.

mod bindless;
mod gpu;
mod indirect;
mod misc;
mod rays;
mod report;

use std::process::ExitCode;

use gpu::Gpu;
use report::Report;

const USAGE: &str = "pyreal-gpu-probe [--json <path>]";

fn main() -> ExitCode {
    let mut json = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--json" => match args.next() {
                Some(p) => json = Some(p),
                None => {
                    eprintln!("--json needs a path\n{USAGE}");
                    return ExitCode::from(2);
                }
            },
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument {other:?}\n{USAGE}");
                return ExitCode::from(2);
            }
        }
    }

    let gpu = match Gpu::new() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let mut r = Report::new(&gpu);
    bindless::run(&gpu, &mut r);
    indirect::run(&gpu, &mut r);
    rays::run(&gpu, &mut r);
    misc::timestamps(&gpu, &mut r);
    misc::limits(&gpu, &mut r);
    misc::buffers(&gpu, &mut r);
    misc::immediates(&gpu, &mut r);
    misc::subgroups(&gpu, &mut r);
    misc::formats(&gpu, &mut r);
    misc::others(&gpu, &mut r);

    r.print();
    if let Some(path) = json {
        if let Err(e) = std::fs::write(&path, r.json(&gpu)) {
            eprintln!("cannot write {path}: {e}");
            return ExitCode::FAILURE;
        }
        println!("\nwrote {path}");
    }
    if r.failures() > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
