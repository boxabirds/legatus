//! Write a capture to a new timestamped file. An existing name is refused and never truncated.
use crate::capture::Capture;
use legatus_proxy::time::WallClock;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum WriteError {
    /// The target name exists; nothing was opened for write.
    Exists(PathBuf),
    /// The capture has no scan stamp.
    NotScanned,
    Io(std::io::Error),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::Exists(path) => write!(f, "{} exists and is not overwritten", path.display()),
            WriteError::NotScanned => f.write_str("the capture has no scan stamp"),
            WriteError::Io(e) => write!(f, "write failed: {e}"),
        }
    }
}

impl std::error::Error for WriteError {}

const MS_PER_SECOND: i64 = 1000;
const SECONDS_PER_DAY: i64 = 86_400;
const SECONDS_PER_HOUR: i64 = 3_600;
const SECONDS_PER_MINUTE: i64 = 60;
/// Days from 0000-03-01 to 1970-01-01 in the proleptic Gregorian calendar.
const DAYS_TO_UNIX_EPOCH: i64 = 719_468;
const DAYS_PER_ERA: i64 = 146_097;

/// `YYYYMMDDTHHMMSSmmmZ` in UTC from milliseconds since the Unix epoch.
pub fn utc_stamp(unix_ms: i64) -> String {
    let secs = unix_ms.div_euclid(MS_PER_SECOND);
    let ms = unix_ms.rem_euclid(MS_PER_SECOND);
    let days = secs.div_euclid(SECONDS_PER_DAY);
    let in_day = secs.rem_euclid(SECONDS_PER_DAY);
    // Civil date from days (Howard Hinnant's algorithm).
    let z = days + DAYS_TO_UNIX_EPOCH;
    let era = z.div_euclid(DAYS_PER_ERA);
    let doe = z.rem_euclid(DAYS_PER_ERA);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let (hh, mm, ss) = (in_day / SECONDS_PER_HOUR, in_day % SECONDS_PER_HOUR / SECONDS_PER_MINUTE, in_day % SECONDS_PER_MINUTE);
    format!("{y:04}{m:02}{d:02}T{hh:02}{mm:02}{ss:02}{ms:03}Z")
}

/// Write `<base>-<UTC timestamp>.json` in `dir` with create-new semantics. The capture must carry a
/// scan stamp. An existing file is left byte for byte as it was.
pub fn write_capture(dir: &Path, base: &str, c: &Capture, wall: &dyn WallClock) -> Result<PathBuf, WriteError> {
    if c.scan_stamp.is_none() {
        return Err(WriteError::NotScanned);
    }
    let path = dir.join(format!("{base}-{}.json", utc_stamp(wall.now().unix_ms)));
    let mut file = match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Err(WriteError::Exists(path)),
        Err(e) => return Err(WriteError::Io(e)),
    };
    let text = serde_json::to_string_pretty(c).map_err(|e| WriteError::Io(std::io::Error::other(e)))?;
    file.write_all(text.as_bytes()).map_err(WriteError::Io)?;
    file.write_all(b"\n").map_err(WriteError::Io)?;
    Ok(path)
}
