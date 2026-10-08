//! `legatus-capture redact <raw.json> <out-dir> <base>`: redact a raw capture, scan it, and write a
//! new timestamped file. A leak makes it exit 1 with the field path and the length only, and
//! nothing is written. An existing target name is refused.
use legatus_proxy::net::wall_system::SystemWallClock;
use legatus_proxy::time::WallClock;
use legatus_testkit::redact::scan::{scan_and_stamp, LEAK_SCAN_MIN_CHARS};
use legatus_testkit::redact::write::write_capture;
use legatus_testkit::redact::{parse_raw, Redactor};
use std::path::Path;
use std::process::ExitCode;

const EXIT_LEAK: u8 = 1;
const EXIT_USAGE: u8 = 2;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let [_, command, raw, dir, base] = args.as_slice() else {
        eprintln!("usage: legatus-capture redact <raw.json> <out-dir> <base>");
        return ExitCode::from(EXIT_USAGE);
    };
    if command != "redact" {
        eprintln!("usage: legatus-capture redact <raw.json> <out-dir> <base>");
        return ExitCode::from(EXIT_USAGE);
    }
    let Ok(bytes) = std::fs::read(raw) else {
        eprintln!("cannot read {raw}");
        return ExitCode::from(EXIT_USAGE);
    };
    let capture = match parse_raw(&bytes) {
        Ok(capture) => capture,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let wall = SystemWallClock;
    let seed = u64::try_from(wall.now().unix_ms).unwrap_or(1) ^ u64::from(std::process::id());
    let redactor = Redactor::new_run(seed);
    let redacted = match redactor.redact(&capture) {
        Ok(redacted) => redacted,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    drop(redactor);
    let stamped = match scan_and_stamp(&capture, &redacted, LEAK_SCAN_MIN_CHARS) {
        Ok(stamped) => stamped,
        Err(leak) => {
            eprintln!("{leak}");
            return ExitCode::from(EXIT_LEAK);
        }
    };
    match write_capture(Path::new(dir), base, &stamped, &wall) {
        Ok(path) => {
            println!("{}", path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(EXIT_LEAK)
        }
    }
}
