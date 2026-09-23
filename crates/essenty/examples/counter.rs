//! End-to-end counter demo over all four core crates.
//!
//! Run with: `cargo run -p essenty --example counter`

use essenty::{BackDispatcher, InstanceKeeper, LifecycleRegistry, StateKeeper};

fn main() {
    // --- Lifecycle -------------------------------------------------------
    let lifecycle = LifecycleRegistry::new();
    let _guard = lifecycle.subscribe(|state| println!("[lifecycle] now {state:?}"));
    lifecycle.create().unwrap();
    lifecycle.start().unwrap();
    lifecycle.resume().unwrap();

    // --- State -----------------------------------------------------------
    let mut keeper = StateKeeper::new();
    keeper.register("counter", || 41_u32.to_le_bytes().to_vec()).unwrap();
    let snapshot = keeper.save().unwrap();
    println!("[state] saved {} key(s)", snapshot.len());

    let mut restored = StateKeeper::with_restored(snapshot);
    let bytes = restored.consume_bytes("counter").expect("restored counter");
    let counter = u32::from_le_bytes(bytes.try_into().expect("u32 bytes"));
    println!("[state] restored counter = {counter}");

    // --- Instances -------------------------------------------------------
    let mut instances = InstanceKeeper::new();
    let model = instances.get_or_create("model", || format!("counter={counter}")).unwrap();
    println!("[instances] retained model = {model}");

    // --- Back ------------------------------------------------------------
    let mut back = BackDispatcher::new();
    back.register(0, true, |event| println!("[back] {event:?}"));
    println!("[back] pressed, handled = {}", back.back());
    println!("[back] gesture start, handled = {}", back.predictive_start());
    back.predictive_progress(0.5).expect("gesture in flight");
    back.predictive_invoke().expect("gesture in flight");
    println!("done");
}
