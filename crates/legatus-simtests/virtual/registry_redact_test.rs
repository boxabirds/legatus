//! Story 125 unit tests: error text holds no value, and warnings reach every place.
use legatus_proxy::config::read::{parse_registry_text, print_errors};
use legatus_proxy::config::registry::*;
use legatus_proxy::config::validate::{report_warnings, validate_registry, WarningSink};
use legatus_proxy::lifecycle::start::{AdminWarnings, LoadReporter};
use legatus_testkit::virt::MemorySink;
use std::sync::{Arc, Mutex};

/// Room of the in-memory log sink in these tests.
const SINK_CAPACITY: usize = 16;
const CANARY_SECRET: &str = "sk-ant-api03-Zk3QfN8vLm2PxTcR9wYbHd7sUe1AoJqGiV5nXtKzB4";
const CANARY_PROMPT: &str = "Summarise the confidential merger memo for Halvorsen Biotech and list the unannounced acquisition price";

fn everything(text: &str) -> String {
    let doc = parse_registry_text(text, "registry.yaml").unwrap();
    let mut out = Vec::new();
    print_errors(&validate_registry(&doc).errors, &mut out);
    String::from_utf8(out).unwrap()
}

#[test]
fn tc10_a_canary_secret_and_a_canary_prompt_in_values_never_reach_an_error_text() {
    let text = format!(
        "version: 1\nnodes: {CANARY_SECRET}\naliases:\n  a: {CANARY_PROMPT}\nsettings: \"{CANARY_PROMPT}\"\nharnesses:\n  - name: {{ k: {CANARY_SECRET} }}\n    session_header: [{CANARY_PROMPT}]\n"
    );
    let shown = everything(&text);
    assert!(shown.contains("bad_type at nodes: Expected map, found string."), "{shown}");
    assert!(!shown.contains("sk-ant"), "{shown}");
    assert!(!shown.contains("Halvorsen") && !shown.contains("merger"), "{shown}");
}

#[test]
fn tc10_a_wrong_type_text_names_only_the_kinds() {
    let e = error_wrong_type("nodes.n1.slots", "integer", FoundKind::Str);
    assert_eq!(e.text, "Expected integer, found string.");
    for (kind, name) in [(FoundKind::Missing, "nothing"), (FoundKind::Null, "null"), (FoundKind::Bool, "boolean"), (FoundKind::Float, "number"), (FoundKind::List, "list"), (FoundKind::Map, "map")] {
        assert_eq!(kind.name(), name);
    }
}

#[test]
fn tc10_a_ref_error_carries_the_reference_name_only() {
    // Story 135 builds the name from a reference that passed its grammar; the crate-private
    // builder is not reachable from outside, so this test goes through the public constructor.
    let e = error_fixed(ErrorCode::SecretUnresolved, "nodes.h.auth.key_ref", "Secret reference does not resolve.");
    assert_eq!(e.to_string(), "registry error secret_unresolved at nodes.h.auth.key_ref: Secret reference does not resolve.");
}

fn warning(code: WarningCode, path: &str) -> RegistryWarning {
    warning_fixed(code, path, "Declared value differs from the tested one.")
}

struct SharedBuf(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for SharedBuf {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn reporter() -> (LoadReporter, Arc<Mutex<Vec<u8>>>, Arc<MemorySink>, Arc<AdminWarnings>) {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::new(MemorySink::new(SINK_CAPACITY));
    let state = Arc::new(AdminWarnings::default());
    let r = LoadReporter { out: Mutex::new(Box::new(SharedBuf(buf.clone()))), log: sink.clone(), state: state.clone() };
    (r, buf, sink, state)
}

#[test]
fn tc12_warnings_only_gives_a_loaded_registry_and_each_warning_reaches_terminal_event_and_admin_state() {
    let (reporter, buf, sink, state) = reporter();
    let ws = vec![warning(WarningCode::FlagMismatch, "nodes.a.engine_flags.np"), warning(WarningCode::RestartRequired, "settings.listen")];
    report_warnings(&ws, &reporter);
    let terminal = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
    assert_eq!(terminal.lines().count(), 2, "{terminal}");
    assert!(terminal.contains("warning flag_mismatch at nodes.a.engine_flags.np: Declared value differs from the tested one."));
    assert_eq!(sink.len(), 2, "one warning event each");
    assert_eq!(state.get(), ws);
}

#[test]
fn tc12_no_warning_stores_an_empty_list_and_clears_an_old_one() {
    let (reporter, buf, sink, state) = reporter();
    report_warnings(&[warning(WarningCode::FlagMismatch, "a")], &reporter);
    assert_eq!(state.get().len(), 1);
    report_warnings(&[], &reporter);
    assert!(state.get().is_empty());
    assert_eq!(sink.len(), 1);
    assert_eq!(String::from_utf8(buf.lock().unwrap().clone()).unwrap().lines().count(), 1);
}

struct Counting(Mutex<usize>);
impl WarningSink for Counting {
    fn warn(&self, _w: &RegistryWarning) {
        *self.0.lock().unwrap() += 1;
    }
}

#[test]
fn tc12_a_sink_that_drops_everything_does_not_stop_the_list() {
    let sink = legatus_testkit::faults::sink::FaultySink::new();
    sink.set_fault(Some(legatus_testkit::stubs::scenario::SinkFault::Fail));
    let state = Arc::new(AdminWarnings::default());
    let reporter = LoadReporter { out: Mutex::new(Box::new(std::io::sink())), log: Arc::new(sink), state: state.clone() };
    let ws = vec![warning(WarningCode::FlagMismatch, "a"), warning(WarningCode::RestartRequired, "b")];
    report_warnings(&ws, &reporter);
    assert_eq!(state.get(), ws, "the admin state is kept although the log refuses");
    let counting = Counting(Mutex::new(0));
    report_warnings(&ws, &counting);
    assert_eq!(*counting.0.lock().unwrap(), 2);
}
