//! The check list: every check runs on the whole file, and the result is one sorted error list.
use crate::config::registry::*;
use crate::config::schema::{check_shape, check_version, ROOT_PATH};
use std::panic::{catch_unwind, AssertUnwindSafe};

/// One check over the whole tree. Later stories (136, 153, 165, 135) add theirs to `checks()`.
pub trait RegistryCheck {
    fn check(&self, doc: &RawRegistry, out: &mut ValidationReport);
}

/// The registered checks, run after the version and shape checks of this story.
pub fn checks() -> Vec<Box<dyn RegistryCheck>> {
    #[allow(unused_mut)]
    let mut list: Vec<Box<dyn RegistryCheck>> = vec![
        Box::new(crate::config::checks_node::NodeCheckShape),
        Box::new(crate::config::checks_node::NodeCheckFlags),
        Box::new(crate::config::checks_node::NodeCheckWarnings),
        Box::new(crate::config::checks_alias::AliasCheck),
    ];
    #[cfg(feature = "test-hooks")]
    list.push(Box::new(test_hook::EnvWarningCheck));
    list
}

/// Feature `test-hooks` only: lets the integration test of the real binary raise one load warning
/// before any story registers a check that does (stories 136, 165).
#[cfg(feature = "test-hooks")]
mod test_hook {
    use super::*;
    /// Environment variable that holds the path of the warning.
    pub const WARNING_PATH_ENV: &str = "LEGATUS_TEST_WARNING_PATH";
    pub struct EnvWarningCheck;
    impl RegistryCheck for EnvWarningCheck {
        fn check(&self, _doc: &RawRegistry, out: &mut ValidationReport) {
            if let Ok(path) = std::env::var(WARNING_PATH_ENV) {
                out.warnings.push(warning_fixed(WarningCode::FlagMismatch, &path, "Declared value differs from the tested one."));
            }
        }
    }
}

/// Pure, no I/O: run the version check, the shape check and every registered check.
pub fn validate_registry(doc: &RawRegistry) -> ValidationReport {
    validate_with(doc, &checks())
}

/// Same as `validate_registry` with an explicit check list (the extension point under test).
pub fn validate_with(doc: &RawRegistry, list: &[Box<dyn RegistryCheck>]) -> ValidationReport {
    let mut report = ValidationReport::default();
    check_version(&doc.0, &mut report);
    check_shape(&doc.0, &mut report);
    for check in list {
        let errors_before = report.errors.len();
        let warnings_before = report.warnings.len();
        let outcome = catch_unwind(AssertUnwindSafe(|| check.check(doc, &mut report)));
        if outcome.is_err() {
            report.errors.truncate(errors_before);
            report.warnings.truncate(warnings_before);
            report.errors.push(error_fixed(ErrorCode::InternalCheckFailed, ROOT_PATH, TEXT_CHECK_PANICKED));
        }
    }
    collect(&mut report);
    report
}

/// An `inline_secret` error at a path replaces an `unknown_field` error at the same path, then
/// errors and warnings are sorted by path, then code (PROPOSED order).
fn collect(report: &mut ValidationReport) {
    let secret_paths: Vec<String> = report.errors.iter().filter(|e| e.code == ErrorCode::InlineSecret).map(|e| e.path.clone()).collect();
    report.errors.retain(|e| !(e.code == ErrorCode::UnknownField && secret_paths.contains(&e.path)));
    report.errors.sort_by(|a, b| (a.path.as_str(), a.code.as_str()).cmp(&(b.path.as_str(), b.code.as_str())));
    report.warnings.sort_by(|a, b| (a.path.as_str(), a.code.as_str()).cmp(&(b.path.as_str(), b.code.as_str())));
}

/// Where load warnings go (terminal, event log, admin state). A failing sink never turns a
/// warning into an error, so `warn` returns nothing.
pub trait WarningSink {
    fn warn(&self, w: &RegistryWarning);
    /// Called once after the last warning with the whole list; an empty list is stored as empty.
    fn done(&self, _all: &[RegistryWarning]) {}
}

pub fn report_warnings(ws: &[RegistryWarning], sink: &dyn WarningSink) {
    for w in ws {
        sink.warn(w);
    }
    sink.done(ws);
}
