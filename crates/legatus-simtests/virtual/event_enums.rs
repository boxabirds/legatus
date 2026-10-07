//! TC-24: wire values of the event enums.
use legatus_common::event::{Outcome, PlacementLabel, ShutdownSignal, SystemEventKind};
use serde_json::{from_str, to_string};

fn wire<T: serde::Serialize>(v: &T) -> String {
    to_string(v).unwrap().trim_matches('"').to_string()
}

#[test]
fn outcome_has_the_11_wire_values() {
    use Outcome::*;
    let all = [Ok, NodeErrorStatus, NodeConnectFailed, NodeStreamCut, ClientClosed, HoldLimit, NoNode, ContextGuard, BadRequest, Unauthorized, ProxyError];
    let want = ["ok", "node_error_status", "node_connect_failed", "node_stream_cut", "client_closed", "hold_limit", "no_node", "context_guard", "bad_request", "unauthorized", "proxy_error"];
    assert_eq!(all.len(), 11);
    for (v, w) in all.iter().zip(want) {
        assert_eq!(wire(v), w);
        assert_eq!(from_str::<Outcome>(&format!("\"{w}\"")).unwrap(), *v);
    }
    assert!(from_str::<Outcome>("\"nonsense\"").is_err());
}

#[test]
fn placement_label_has_8_values() {
    use PlacementLabel::*;
    let all = [Affinity, NewWarmSlot, NewEvictIdle, LeastLoaded, HeldThenPlaced, Moved, SiblingSpill, None];
    let want = ["affinity", "new_warm_slot", "new_evict_idle", "least_loaded", "held_then_placed", "moved", "sibling_spill", "none"];
    for (v, w) in all.iter().zip(want) {
        assert_eq!(wire(v), w);
        assert_eq!(from_str::<PlacementLabel>(&format!("\"{w}\"")).unwrap(), *v);
    }
    assert!(from_str::<PlacementLabel>("\"nonsense\"").is_err());
}

#[test]
fn system_event_kind_has_17_values() {
    use SystemEventKind::*;
    let all = [Start, Ready, RegistryLoaded, RegistryRejected, NodeState, LogPaused, LogResumed, Shutdown, HoldStart, HoldEnd, HoldRefused, Moved, AffinityModeChanged, TruncationDetected, Warning, Finding, CalibrationDone];
    let want = ["start", "ready", "registry_loaded", "registry_rejected", "node_state", "log_paused", "log_resumed", "shutdown", "hold_start", "hold_end", "hold_refused", "moved", "affinity_mode_changed", "truncation_detected", "warning", "finding", "calibration_done"];
    assert_eq!(all.len(), 17);
    for (v, w) in all.iter().zip(want) {
        assert_eq!(wire(v), w);
        assert_eq!(from_str::<SystemEventKind>(&format!("\"{w}\"")).unwrap(), *v);
    }
    assert!(from_str::<SystemEventKind>("\"nonsense\"").is_err());
}

#[test]
fn shutdown_signal_is_term_and_int() {
    assert_eq!(wire(&ShutdownSignal::Term), "term");
    assert_eq!(wire(&ShutdownSignal::Int), "int");
    assert!(from_str::<ShutdownSignal>("\"kill\"").is_err());
}
