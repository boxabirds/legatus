//! Virtual-time test kit (story 144): paused runtime, stepwise driver, fakes, in-memory serving.
pub mod driver;
pub mod duplex;
pub mod fake_transport;
pub mod mem_sink;
pub mod offset;
pub mod runtime;
pub mod wall;

pub use driver::Driver;
pub use duplex::{serve_duplex, DuplexClient};
pub use fake_transport::{FakeTransport, RecordedRequest, Script};
pub use mem_sink::MemorySink;
pub use offset::{MsOffset, OffsetError};
pub use runtime::virtual_runtime;
pub use wall::SimWall;
