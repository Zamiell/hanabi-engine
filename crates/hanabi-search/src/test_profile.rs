//! Opt-in replay profiling; absent from production builds. Inclusive times
//! overlap; exclusive times subtract nested instrumented scopes.

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashSet},
    time::{Duration, Instant},
};

#[derive(Default)]
struct Row {
    calls: u64,
    inclusive: Duration,
    exclusive: Duration,
}

#[derive(Default)]
struct Profile {
    children: Vec<Duration>,
    rows: BTreeMap<&'static str, Row>,
    replay_misses: HashSet<crate::h_group::ReplayMemoKey>,
    hits: u64,
    misses: u64,
    repeated_misses: u64,
    untracked_misses: u64,
    peak_entries: usize,
    flushes: u64,
}

pub(crate) fn replay_lookup(key: &crate::h_group::ReplayMemoKey, hit: bool) {
    PROFILE.with_borrow_mut(|profile| {
        let Some(profile) = profile else { return };
        if hit {
            profile.hits += 1;
        } else {
            profile.misses += 1;
            if profile.replay_misses.contains(key) {
                profile.repeated_misses += 1;
            } else if profile.replay_misses.len() < 8_192 {
                profile.replay_misses.insert(key.clone());
            } else {
                profile.untracked_misses += 1;
            }
        }
    });
}

pub(crate) fn replay_peak(entries: usize) {
    PROFILE.with_borrow_mut(|profile| {
        if let Some(profile) = profile {
            profile.peak_entries = profile.peak_entries.max(entries);
        }
    });
}

pub(crate) fn replay_flush() {
    PROFILE.with_borrow_mut(|profile| {
        if let Some(profile) = profile {
            profile.flushes += 1;
        }
    });
}

thread_local! {
    static PROFILE: RefCell<Option<Profile>> = const { RefCell::new(None) };
}

pub(crate) fn start() {
    PROFILE.with_borrow_mut(|profile| {
        assert!(profile.is_none(), "profiling sessions must not overlap");
        *profile = Some(Profile::default());
    });
}

pub(crate) struct Span(Option<(&'static str, Instant)>);

pub(crate) fn span(label: &'static str) -> Span {
    Span(PROFILE.with_borrow_mut(|profile| {
        let profile = profile.as_mut()?;
        profile.children.push(Duration::ZERO);
        Some((label, Instant::now()))
    }))
}

impl Drop for Span {
    fn drop(&mut self) {
        let Some((label, started)) = self.0 else {
            return;
        };
        let elapsed = started.elapsed();
        PROFILE.with_borrow_mut(|profile| {
            let profile = profile.as_mut().expect("active profiling session");
            let children = profile.children.pop().expect("balanced profiling scopes");
            if let Some(parent) = profile.children.last_mut() {
                *parent += elapsed;
            }
            let row = profile.rows.entry(label).or_default();
            row.calls += 1;
            row.inclusive += elapsed;
            row.exclusive += elapsed.saturating_sub(children);
        });
    }
}

pub(crate) fn finish(seed: &str, turn: u32, elapsed: Duration) {
    let profile = PROFILE
        .with_borrow_mut(Option::take)
        .expect("active profiling session");
    assert!(profile.children.is_empty());
    eprintln!(
        "REPLAY_MEMO\t{seed}\t{turn}\thits={}\tmisses={}\trepeated_misses={}\tuntracked_misses={}\tpeak_entries={}\tflushes={}",
        profile.hits,
        profile.misses,
        profile.repeated_misses,
        profile.untracked_misses,
        profile.peak_entries,
        profile.flushes
    );
    eprintln!(
        "REPLAY_PROFILE\t{seed}\t{turn}\ttotal\t1\t{:.6}\t{:.6}",
        elapsed.as_secs_f64(),
        elapsed.as_secs_f64()
    );
    for (label, row) in profile.rows {
        eprintln!(
            "REPLAY_PROFILE\t{seed}\t{turn}\t{label}\t{}\t{:.6}\t{:.6}",
            row.calls,
            row.inclusive.as_secs_f64(),
            row.exclusive.as_secs_f64()
        );
    }
}

#[test]
fn nested_scopes_count_calls_without_double_counting_time() {
    start();
    {
        let _outer = span("outer");
        let _inner = span("inner");
    }
    PROFILE.with_borrow(|profile| {
        let profile = profile.as_ref().unwrap();
        let outer = &profile.rows["outer"];
        let inner = &profile.rows["inner"];
        assert_eq!((outer.calls, inner.calls), (1, 1));
        assert_eq!(outer.exclusive + inner.inclusive, outer.inclusive);
        assert_eq!(inner.exclusive, inner.inclusive);
    });
    finish("unit", 1, Duration::ZERO);
    drop(span("disabled"));
    PROFILE.with_borrow(|profile| assert!(profile.is_none()));
}
