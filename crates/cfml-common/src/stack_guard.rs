//! Native-stack headroom check.
//!
//! A stack overflow in Rust is an `abort()`, not a panic: it takes the whole
//! multi-threaded server down with one request. The VM bounds CFML call depth
//! (2,500 frames) but a native frame's size is not a constant — a debug build's
//! `execute_function_with_args` frame is hundreds of KB — so a frame-count
//! ceiling alone cannot promise the native stack holds. This module lets the
//! recursive walkers (the VM call path, the JSON writer) ask how much of the
//! current thread's stack is left and fail with a catchable CFML error instead.
//!
//! Each thread that runs CFML calls [`init`] with its stack size once, at the
//! top of the thread (`run()`'s 64 MB VM thread, every cfthread, the server's
//! blocking-pool threads). A thread that never called `init` (wasm, tests) gets
//! no check: [`has_headroom`] answers `true` there.
//!
//! The stack grows downwards on every target RustCFML builds for; the address
//! of a local at `init` is within a few KB of the stack's top.

use std::cell::Cell;

thread_local! {
    /// Lowest usable stack address for this thread, or 0 when unknown.
    static STACK_FLOOR: Cell<usize> = const { Cell::new(0) };
}

/// Record the current thread's stack floor from its known `stack_size`. Call
/// once, as the first thing the thread does.
pub fn init(stack_size: usize) {
    let probe = 0u8;
    let here = &probe as *const u8 as usize;
    STACK_FLOOR.with(|f| f.set(here.saturating_sub(stack_size)));
}

/// Bytes of native stack left on this thread, or `None` when the thread never
/// called [`init`].
#[inline]
pub fn remaining() -> Option<usize> {
    let probe = 0u8;
    let sp = &probe as *const u8 as usize;
    STACK_FLOOR.with(|f| {
        let floor = f.get();
        if floor == 0 { None } else { Some(sp.saturating_sub(floor)) }
    })
}

/// `true` when at least `bytes` of native stack remain (or when the thread's
/// stack is unknown, in which case nothing is guarded).
#[inline]
pub fn has_headroom(bytes: usize) -> bool {
    remaining().map(|r| r > bytes).unwrap_or(true)
}

/// Headroom the VM keeps below a CFML call: enough for the deepest native
/// path a single CFML frame can take (a builtin, an error unwind, a catch
/// handler). Debug frames are an order of magnitude larger than release ones.
pub const CALL_RED_ZONE: usize = if cfg!(debug_assertions) { 8 << 20 } else { 1 << 20 };

/// Headroom a recursive value walker (serializer, copier) keeps per level.
pub const WALK_RED_ZONE: usize = if cfg!(debug_assertions) { 1 << 20 } else { 256 << 10 };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uninitialised_thread_is_unguarded() {
        std::thread::spawn(|| {
            assert_eq!(remaining(), None);
            assert!(has_headroom(usize::MAX));
        })
        .join()
        .unwrap();
    }

    #[test]
    fn initialised_thread_reports_decreasing_headroom() {
        std::thread::Builder::new()
            .stack_size(4 << 20)
            .spawn(|| {
                init(4 << 20);
                let top = remaining().unwrap();
                assert!(top <= 4 << 20 && top > 3 << 20, "top = {top}");
                fn deeper(n: usize) -> usize {
                    let pad = [0u8; 4096];
                    std::hint::black_box(&pad);
                    if n == 0 { remaining().unwrap() } else { deeper(n - 1) }
                }
                let lower = deeper(16);
                assert!(lower < top, "{lower} < {top}");
                assert!(has_headroom(1 << 20));
                assert!(!has_headroom(4 << 20));
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
