//! Story 127: the reuse reader allocates nothing on a good reading. A counting allocator counts
//! the allocations made by the current thread only, so other tests running at once do not count.
use legatus_common::engine::*;
use legatus_common::protocol::Protocol;
use legatus_proxy::engine::reuse::extract_reuse;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|c| c.set(c.get() + 1));
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

fn allocations_during(work: impl FnOnce()) -> usize {
    let before = ALLOCATIONS.with(Cell::get);
    work();
    ALLOCATIONS.with(Cell::get) - before
}

const TAIL: &str = "{\"id\":\"x\",\"choices\":[{\"message\":{\"content\":\"hello\"}}],\"timings\":{\"cache_n\":5484,\"prompt_n\":516},\"usage\":{\"prompt_tokens_details\":{\"cached_tokens\":300}}}";

#[test]
fn a_good_reading_and_an_absent_field_allocate_nothing() {
    let view = UsageView { json_tail: TAIL.as_bytes(), protocol: Protocol::OpenAiChat, stream: false };
    let mut readings = Vec::with_capacity(4);
    let count = allocations_during(|| {
        readings.push(extract_reuse(ReuseFieldName::TimingsCacheN, &view));
        readings.push(extract_reuse(ReuseFieldName::PromptTokensDetailsCachedTokens, &view));
        readings.push(extract_reuse(ReuseFieldName::InputTokensDetailsCachedTokens, &view));
    });
    assert_eq!(count, 0, "allocations during three readings");
    assert_eq!(readings[0], ReuseReading::Reused { cached_tokens: 5484, field: ReuseFieldName::TimingsCacheN });
    assert_eq!(readings[2], ReuseReading::Unknown(UnknownReason::FieldAbsent));
}

#[test]
fn the_counter_does_count_so_a_zero_is_not_an_artefact() {
    assert!(allocations_during(|| drop(std::hint::black_box(String::from("counted")))) >= 1);
}

#[test]
fn nested_skipped_values_malformed_values_and_non_json_allocate_nothing_either() {
    let tails = [
        "{\"choices\":[{\"a\":1,\"b\":\"x\\\"y\"}],\"timings\":{\"cache_n\":5}}",
        "{\"timings\":{\"cache_n\":\"5\"}}",
        "{\"timings\":{\"cache_n\":null}}",
        "{\"timings\":{\"cache_n\":-5}}",
        "data: {\"timings\":{\"cache_n\":5}}",
        "not json at all",
        "{\"timings\":",
        "",
    ];
    for tail in tails {
        let view = UsageView { json_tail: tail.as_bytes(), protocol: Protocol::OpenAiChat, stream: false };
        let count = allocations_during(|| {
            let _ = std::hint::black_box(extract_reuse(ReuseFieldName::TimingsCacheN, &view));
        });
        assert_eq!(count, 0, "{tail:?}");
    }
}
