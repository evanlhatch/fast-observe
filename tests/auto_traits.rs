//! Compile-time contracts: thread-bound guards must never be Send/Sync.
//!
//! Every guard below pops a thread-local stack at a stored depth on drop —
//! a cross-thread drop would corrupt the receiving thread's stack. The
//! `PhantomData<*const ()>` marker in each guard turns that into a compile
//! error; these assertions keep the contract from regressing.

use static_assertions::assert_not_impl_any;

assert_not_impl_any!(fast_observe::profiling::ScopeGuard: Send, Sync);
assert_not_impl_any!(fast_observe::profiling::FunctionScopeGuard: Send, Sync);

#[cfg(feature = "instant")]
assert_not_impl_any!(fast_observe::profiling::instant::InstantGuard: Send, Sync);

// Native builds see the hand-written `web_wrap` ZST stub (the real type is
// wasm-only) — the stub carries the same marker.
assert_not_impl_any!(fast_observe::profiling::web_wrap::WebMarkGuard: Send, Sync);
