//! susz-olean: Lean `.olean` files straight to a Susz file.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

const USAGE: &str = "usage: susz-olean [-L DIR]... MODULE... -o OUT.susz [--no-index] [--threads N] [--jobs N] [--quiet]

Loads MODULE and its imports from the .olean files found on -L DIR,
LEAN_PATH and LEAN_SYSROOT (as olean-export does), and packs every
non-internal constant into OUT.susz: the file `olean-export MODULE... -f ndjson
| susz pack` writes, with no ndjson in between. --jobs: threads decoding the
olean files (all cores); --threads: threads writing the Susz file (all cores).
Exits 0 written, 1 the export is not valid, 2 cannot (olean files, I/O, a limit).";

fn main() -> ExitCode {
    let mut search: Vec<PathBuf> = Vec::new();
    let mut roots: Vec<String> = Vec::new();
    let mut out: Option<String> = None;
    let mut opts = susz::pack::Options {
        index: true,
        threads: susz::pack::all_cores(),
    };
    let mut jobs = susz::pack::all_cores();
    let mut quiet = false;
    let mut args = std::env::args().skip(1);
    let num = |v: Option<String>| v.and_then(|s| s.parse::<usize>().ok()).filter(|&n| n > 0);
    while let Some(a) = args.next() {
        let bad = match a.as_str() {
            "-L" => args.next().map(|d| search.push(d.into())).is_none(),
            "-o" => {
                out = args.next();
                out.is_none()
            }
            "--no-index" => {
                opts.index = false;
                false
            }
            "--threads" => num(args.next()).map(|n| opts.threads = n).is_none(),
            "--jobs" => num(args.next()).map(|n| jobs = n).is_none(),
            "--quiet" => {
                quiet = true;
                false
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            s if s.starts_with('-') => true,
            _ => {
                roots.push(a);
                false
            }
        };
        if bad {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    }
    let Some(out) = out.filter(|_| !roots.is_empty()) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    search.extend(olean_export::search_path());
    if search.is_empty() {
        eprintln!("susz-olean: no search path: pass -L DIR or run under `lake env`");
        return ExitCode::from(2);
    }
    let start = Instant::now();
    let roots: Vec<&str> = roots.iter().map(String::as_str).collect();
    let env = match olean_export::Env::load_with(&search, &roots, jobs, &|_, _| {}) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("susz-olean: olean-export: {e}");
            return ExitCode::from(2);
        }
    };
    let loaded = start.elapsed();
    match susz_olean::write_env(&env, &out, &opts) {
        Ok(st) => {
            if !quiet {
                eprintln!(
                    "{} modules decoded in {:.2}s; {st}; {:.2}s total",
                    env.modules.len(),
                    loaded.as_secs_f64(),
                    start.elapsed().as_secs_f64()
                );
            }
            ExitCode::SUCCESS
        }
        Err(susz_olean::Failure::Pack(e)) => {
            eprintln!("susz-olean: {out}: {e}");
            ExitCode::from(match e.fault {
                susz::pack::Fault::Invalid => 1,
                susz::pack::Fault::Cannot => 2,
            })
        }
        Err(e) => {
            eprintln!("susz-olean: {e}");
            ExitCode::from(2)
        }
    }
}
