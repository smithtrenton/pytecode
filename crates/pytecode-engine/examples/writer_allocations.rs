//! Count writer allocations independently of Criterion timing.
//! `cargo run --release -p pytecode-engine --example writer_allocations -- path/to/corpus.jar`

use pytecode_engine::{parse_class, write_class};
use std::alloc::{GlobalAlloc, Layout, System};
use std::fs::File;
use std::io::Read;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

struct CountingAllocator;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static REQUESTED: AtomicUsize = AtomicUsize::new(0);

fn allocated(size: usize) {
    ALLOCATIONS.fetch_add(1, Relaxed);
    REQUESTED.fetch_add(size, Relaxed);
    let live = LIVE.fetch_add(size, Relaxed) + size;
    PEAK.fetch_max(live, Relaxed);
}

// SAFETY: Every operation forwards the original pointer/layout to System;
// bookkeeping uses only atomics and cannot allocate or unwind.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            allocated(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(pointer, layout, size) };
        if !result.is_null() {
            LIVE.fetch_sub(layout.size(), Relaxed);
            allocated(size);
        }
        result
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: writer_allocations corpus.jar");
    let mut jar = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
    let mut classes = Vec::new();
    for index in 0..jar.len() {
        let mut entry = jar.by_index(index).unwrap();
        if entry.name().ends_with(".class") {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            classes.push(parse_class(&bytes).unwrap());
        }
    }
    for class in &classes {
        std::hint::black_box(write_class(class).unwrap());
    }
    let starting_live = LIVE.load(Relaxed);
    PEAK.store(starting_live, Relaxed);
    ALLOCATIONS.store(0, Relaxed);
    REQUESTED.store(0, Relaxed);
    let mut output_bytes = 0;
    for class in &classes {
        output_bytes += std::hint::black_box(write_class(class).unwrap()).len();
    }
    let allocations = ALLOCATIONS.load(Relaxed);
    let requested = REQUESTED.load(Relaxed);
    let peak = PEAK.load(Relaxed) - starting_live;
    println!(
        "classes={}, output_bytes={output_bytes}, allocations_including_realloc={allocations}, requested_bytes={requested}, peak_additional_live_bytes={peak}",
        classes.len()
    );
}
