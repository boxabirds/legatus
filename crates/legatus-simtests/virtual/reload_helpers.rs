//! Helpers for the reload tests (story 179): a reloader over an in-memory registry source.
use legatus_proxy::config::reload::{manual_trigger, ReloadHub, ReloadObserver, ReloadParts, ReloadState, Reloader, TriggerSender};
use legatus_proxy::config::typed::{Registry, RegistryHandle};
use legatus_proxy::obs::log_sink::LogSink;
use legatus_proxy::sim::SimPoints;
use legatus_proxy::time::WallTime;
use legatus_testkit::virt::{MemorySink, MemorySource, SimWall};
use std::io::Write;
use std::sync::{Arc, Mutex};

const SINK_CAPACITY: usize = 64;

/// One node `a` and one alias `x` for it.
pub const V1: &str = "version: 1\nsettings:\n  listen: 127.0.0.1:18080\nnodes:\n  a:\n    engine: { name: ollama, version: \"0.35\" }\n    model: model-a\n    endpoints: [ { protocol: openai-chat, base_url: \"http://a.invalid:1\" } ]\naliases:\n  x: { nodes: [a] }\n";

/// A second node `b`, alias `x` over both and alias `y` over `b`.
pub const V2: &str = "version: 1\nsettings:\n  listen: 127.0.0.1:18080\nnodes:\n  a:\n    engine: { name: ollama, version: \"0.35\" }\n    model: model-a\n    endpoints: [ { protocol: openai-chat, base_url: \"http://a.invalid:1\" } ]\n  b:\n    engine: { name: ollama }\n    model: model-b\n    endpoints: [ { protocol: openai-chat, base_url: \"http://b.invalid:2\" } ]\naliases:\n  x: { nodes: [a, b] }\n  y: { nodes: [b] }\n";

#[derive(Clone, Default)]
pub struct Buffer(pub Arc<Mutex<Vec<u8>>>);

impl Write for Buffer {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Buffer {
    pub fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

pub struct Harness {
    pub handle: Arc<RegistryHandle>,
    pub source: Arc<MemorySource>,
    pub reloader: Arc<Reloader>,
    pub state: Arc<ReloadState>,
    pub hub: Arc<ReloadHub>,
    pub sink: Arc<MemorySink>,
    pub sim: SimPoints,
    pub out: Buffer,
}

/// A reloader whose first registry (generation 1) is `initial`.
pub fn harness(initial: &str) -> Harness {
    let handle = Arc::new(RegistryHandle::new(Arc::new(Registry::from_text(initial).expect("a valid first registry"))));
    let source = Arc::new(MemorySource::new("registry.yaml", initial));
    let hub = Arc::new(ReloadHub::new());
    let state = Arc::new(ReloadState::default());
    let sink = Arc::new(MemorySink::new(SINK_CAPACITY));
    let sim = SimPoints::new();
    let out = Buffer::default();
    let log: Arc<dyn LogSink> = sink.clone();
    let reloader = Arc::new(Reloader::new(ReloadParts {
        source: source.clone(),
        handle: handle.clone(),
        hub: hub.clone(),
        wall: Arc::new(SimWall::new(WallTime { unix_ms: 1_000 })),
        sink: log,
        sim: sim.clone(),
        state: state.clone(),
        out: Box::new(out.clone()),
    }));
    Harness { handle, source, reloader, state, hub, sink, sim, out }
}

/// A reloader driven by a manual trigger, running in the background.
pub fn running(initial: &str) -> (Harness, TriggerSender, tokio::task::JoinHandle<()>) {
    let h = harness(initial);
    let (trigger, sender) = manual_trigger();
    let task = tokio::spawn(h.reloader.clone().run(trigger));
    (h, sender, task)
}

/// An observer that records the generation it saw and a label, in a shared list.
pub struct Labelled {
    pub label: &'static str,
    pub seen: Arc<Mutex<Vec<String>>>,
    pub panics: bool,
}

impl ReloadObserver for Labelled {
    fn on_reload(&self, old: &Registry, new: &Registry, _diff: &legatus_proxy::config::diff::RegistryDiff) {
        self.seen.lock().unwrap().push(format!("{}:{}->{}", self.label, old.generation, new.generation));
        if self.panics {
            panic!("an observer defect");
        }
    }
}
