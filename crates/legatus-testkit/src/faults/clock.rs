//! Clock jump rule: moves the fake wall clock, never the monotonic clock.
use crate::virt::SimWall;

pub fn apply_clock_jump(wall: &SimWall, delta_ms: i64) {
    wall.jump(delta_ms);
}
