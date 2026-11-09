//! Per-call-site lock contention profiling (`lock-profiling` feature).
//!
//! The traced raw locks wrap `parking_lot`'s raw locks. `lock_api` propagates
//! `#[track_caller]` into the raw `lock*` calls, so each acquisition is keyed by
//! the source location that called `lock`/`read`/`write`. Accessors that hand
//! out guards should use `#[cfg_attr(feature = "lock-profiling", track_caller)]`
//! so their callers are reported instead of the accessor.
//!
//! Recorded per site and lock kind:
//! - acquisitions, and how many found the lock already taken (contended)
//! - total/max time spent waiting when contended
//! - total/max time the guard was held
//!
//! On every contended acquisition, the wait is also blamed on the site that
//! last acquired the lock. For read-write locks this is the last reader *or*
//! writer, so a writer blocked by many readers is blamed on one of them.
//!
//! Stats live in per-thread tables and are merged by [`report`].
//!
//! [`ProfileMode::Full`] adds two `Instant::now` calls and a stats update per
//! acquisition, which lengthens critical sections and inflates contention.
//! [`ProfileMode::WaitOnly`] skips all of that on uncontended acquisitions and
//! records only contended waits and blame, so its wait totals are close to an
//! untraced run.

use std::cmp::Reverse;
use std::fmt::Write as _;
use std::panic::Location;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::Instant;

use parking_lot::lock_api;
use rustc_hash::FxHashMap;

type Site = &'static Location<'static>;
type InnerMutexGuard = <parking_lot::RawMutex as lock_api::RawMutex>::Guard;
type InnerSharedGuard = <parking_lot::RawRwLock as lock_api::RawRwLock>::SharedGuard;
type InnerExclusiveGuard = <parking_lot::RawRwLock as lock_api::RawRwLock>::ExclusiveGuard;

/// Which kind of acquisition a stat row describes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum LockKind {
    /// `SyncMutex::lock`.
    Mutex,
    /// `SyncRwLock::read`.
    Read,
    /// `SyncRwLock::write`.
    Write,
}

impl LockKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Mutex => "mutex",
            Self::Read => "read",
            Self::Write => "write",
        }
    }
}

/// What the traced locks record.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProfileMode {
    /// Acquisition counts, hold times, waits and blame.
    Full,
    /// Only contended waits and blame; uncontended acquisitions are not timed.
    WaitOnly,
}

static HOLD_TIMING: AtomicBool = AtomicBool::new(true);
static STARTED_AT: parking_lot::Mutex<Option<Instant>> = parking_lot::Mutex::new(None);

/// Selects what to record and restarts the measurement window. Call before
/// the measured work starts; switching mid-run mixes modes in one report.
pub fn set_mode(mode: ProfileMode) {
    HOLD_TIMING.store(mode == ProfileMode::Full, Ordering::Relaxed);
    reset();
}

/// The mode selected by [`set_mode`], [`ProfileMode::Full`] by default.
#[must_use]
pub fn mode() -> ProfileMode {
    if HOLD_TIMING.load(Ordering::Relaxed) {
        ProfileMode::Full
    } else {
        ProfileMode::WaitOnly
    }
}

#[inline]
fn hold_start() -> Option<Instant> {
    HOLD_TIMING.load(Ordering::Relaxed).then(Instant::now)
}

fn site_addr(site: Site) -> usize {
    ptr::from_ref(site).addr()
}

/// Aggregated stats for one call site and lock kind.
#[derive(Clone, Copy, Default, Debug)]
pub struct SiteStats {
    /// Successful acquisitions.
    pub acquisitions: u64,
    /// Acquisitions that had to wait.
    pub contended: u64,
    /// Total wait time in nanoseconds.
    pub wait_ns: u64,
    /// Longest single wait in nanoseconds.
    pub max_wait_ns: u64,
    /// Total hold time in nanoseconds.
    pub hold_ns: u64,
    /// Longest single hold in nanoseconds.
    pub max_hold_ns: u64,
}

impl SiteStats {
    const fn merge(&mut self, other: &Self) {
        self.acquisitions += other.acquisitions;
        self.contended += other.contended;
        self.wait_ns += other.wait_ns;
        self.max_wait_ns = if other.max_wait_ns > self.max_wait_ns {
            other.max_wait_ns
        } else {
            self.max_wait_ns
        };
        self.hold_ns += other.hold_ns;
        self.max_hold_ns = if other.max_hold_ns > self.max_hold_ns {
            other.max_hold_ns
        } else {
            self.max_hold_ns
        };
    }
}

#[derive(Clone, Copy, Default)]
struct BlameStats {
    count: u64,
    wait_ns: u64,
}

#[derive(Default)]
struct ThreadStats {
    sites: FxHashMap<(usize, LockKind), (Site, SiteStats)>,
    /// Keyed by (waiter site, holder site address, waiter kind).
    blame: FxHashMap<(usize, usize, LockKind), (Site, BlameStats)>,
}

type ThreadSlot = Arc<parking_lot::Mutex<ThreadStats>>;

static REGISTRY: parking_lot::Mutex<Vec<ThreadSlot>> = parking_lot::Mutex::new(Vec::new());

thread_local! {
    static LOCAL: ThreadSlot = {
        let slot = ThreadSlot::default();
        REGISTRY.lock().push(Arc::clone(&slot));
        slot
    };
}

fn with_local(f: impl FnOnce(&mut ThreadStats)) {
    // Fails only during thread-local destruction; those samples are dropped.
    let _ = LOCAL.try_with(|slot| f(&mut slot.lock()));
}

/// Per-acquisition state carried in the guard.
pub struct TracedGuard<G> {
    inner: G,
    site: Site,
    kind: LockKind,
    /// `None` in [`ProfileMode::WaitOnly`].
    acquired: Option<Instant>,
}

impl<G> TracedGuard<G> {
    #[inline]
    fn release(&self) {
        let Some(acquired) = self.acquired else {
            return;
        };
        let hold_ns = duration_ns(acquired);
        let key = (site_addr(self.site), self.kind);
        let site = self.site;
        with_local(|stats| {
            let entry = &mut stats
                .sites
                .entry(key)
                .or_insert((site, SiteStats::default()))
                .1;
            entry.merge(&SiteStats {
                acquisitions: 1,
                hold_ns,
                max_hold_ns: hold_ns,
                ..SiteStats::default()
            });
        });
    }
}

fn duration_ns(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// Records a contended acquisition and the holder blamed for it.
fn record_wait(waiter: Site, kind: LockKind, holder: usize, started: Instant) {
    let wait_ns = duration_ns(started);
    with_local(|stats| {
        stats
            .sites
            .entry((site_addr(waiter), kind))
            .or_insert((waiter, SiteStats::default()))
            .1
            .merge(&SiteStats {
                contended: 1,
                wait_ns,
                max_wait_ns: wait_ns,
                ..SiteStats::default()
            });
        let entry = &mut stats
            .blame
            .entry((site_addr(waiter), holder, kind))
            .or_insert((waiter, BlameStats::default()))
            .1;
        entry.count += 1;
        entry.wait_ns += wait_ns;
    });
}

/// Tries the fast path, then blocks while timing the wait.
#[inline]
fn acquire<G>(
    site: Site,
    kind: LockKind,
    holder: &AtomicUsize,
    try_lock: impl FnOnce() -> Option<G>,
    lock: impl FnOnce() -> G,
) -> TracedGuard<G> {
    let inner = if let Some(inner) = try_lock() {
        inner
    } else {
        let blamed = holder.load(Ordering::Relaxed);
        let started = Instant::now();
        let inner = lock();
        record_wait(site, kind, blamed, started);
        inner
    };
    holder.store(site_addr(site), Ordering::Relaxed);
    TracedGuard {
        inner,
        site,
        kind,
        acquired: hold_start(),
    }
}

#[inline]
fn try_acquire<G>(
    site: Site,
    kind: LockKind,
    holder: &AtomicUsize,
    try_lock: impl FnOnce() -> Option<G>,
) -> Option<TracedGuard<G>> {
    let inner = try_lock()?;
    holder.store(site_addr(site), Ordering::Relaxed);
    Some(TracedGuard {
        inner,
        site,
        kind,
        acquired: hold_start(),
    })
}

/// A `parking_lot` raw mutex that records call-site timing.
pub struct TracedRawMutex {
    inner: parking_lot::RawMutex,
    /// Address of the last acquiring call site, 0 if never locked.
    holder: AtomicUsize,
}

// SAFETY: Every operation forwards to `parking_lot::RawMutex` with the inner
// guard state it produced, so mutual exclusion and memory ordering are
// parking_lot's. The extra fields are bookkeeping and never gate access.
unsafe impl lock_api::RawMutex for TracedRawMutex {
    const INIT: Self = Self {
        inner: parking_lot::RawMutex::INIT,
        holder: AtomicUsize::new(0),
    };

    type Guard = TracedGuard<InnerMutexGuard>;

    #[inline]
    #[track_caller]
    fn lock(&self) -> Self::Guard {
        acquire(
            Location::caller(),
            LockKind::Mutex,
            &self.holder,
            || self.inner.try_lock(),
            || self.inner.lock(),
        )
    }

    #[inline]
    #[track_caller]
    fn try_lock(&self) -> Option<Self::Guard> {
        try_acquire(Location::caller(), LockKind::Mutex, &self.holder, || {
            self.inner.try_lock()
        })
    }

    #[inline]
    unsafe fn unlock(&self, guard: Self::Guard) {
        guard.release();
        // SAFETY: `guard.inner` came from locking `self.inner` and the caller
        // guarantees this acquisition is still held.
        unsafe { self.inner.unlock(guard.inner) };
    }

    #[inline]
    fn is_locked(&self) -> bool {
        self.inner.is_locked()
    }
}

/// A `parking_lot` raw read-write lock that records call-site timing.
pub struct TracedRawRwLock {
    inner: parking_lot::RawRwLock,
    /// Address of the last acquiring call site (reader or writer), 0 if never locked.
    holder: AtomicUsize,
}

// SAFETY: Every operation forwards to `parking_lot::RawRwLock` with the inner
// guard state it produced, so exclusion and memory ordering are parking_lot's.
// The extra fields are bookkeeping and never gate access.
unsafe impl lock_api::RawRwLock for TracedRawRwLock {
    const INIT: Self = Self {
        inner: parking_lot::RawRwLock::INIT,
        holder: AtomicUsize::new(0),
    };

    type SharedGuard = TracedGuard<InnerSharedGuard>;
    type ExclusiveGuard = TracedGuard<InnerExclusiveGuard>;

    #[inline]
    #[track_caller]
    fn lock_shared(&self) -> Self::SharedGuard {
        acquire(
            Location::caller(),
            LockKind::Read,
            &self.holder,
            || self.inner.try_lock_shared(),
            || self.inner.lock_shared(),
        )
    }

    #[inline]
    #[track_caller]
    fn try_lock_shared(&self) -> Option<Self::SharedGuard> {
        try_acquire(Location::caller(), LockKind::Read, &self.holder, || {
            self.inner.try_lock_shared()
        })
    }

    #[inline]
    unsafe fn unlock_shared(&self, guard: Self::SharedGuard) {
        guard.release();
        // SAFETY: `guard.inner` came from a shared lock of `self.inner` that the
        // caller guarantees is still held.
        unsafe { self.inner.unlock_shared(guard.inner) };
    }

    #[inline]
    #[track_caller]
    fn lock_exclusive(&self) -> Self::ExclusiveGuard {
        acquire(
            Location::caller(),
            LockKind::Write,
            &self.holder,
            || self.inner.try_lock_exclusive(),
            || self.inner.lock_exclusive(),
        )
    }

    #[inline]
    #[track_caller]
    fn try_lock_exclusive(&self) -> Option<Self::ExclusiveGuard> {
        try_acquire(Location::caller(), LockKind::Write, &self.holder, || {
            self.inner.try_lock_exclusive()
        })
    }

    #[inline]
    unsafe fn unlock_exclusive(&self, guard: Self::ExclusiveGuard) {
        guard.release();
        // SAFETY: `guard.inner` came from an exclusive lock of `self.inner` that
        // the caller guarantees is still held.
        unsafe { self.inner.unlock_exclusive(guard.inner) };
    }

    #[inline]
    fn is_locked(&self) -> bool {
        self.inner.is_locked()
    }

    #[inline]
    fn is_locked_exclusive(&self) -> bool {
        self.inner.is_locked_exclusive()
    }
}

/// A `parking_lot` raw condvar paired with [`TracedRawMutex`].
///
/// Time spent waiting on the condvar is not lock contention: the hold before
/// the wait is recorded as one acquisition and the re-acquired lock starts a
/// fresh hold.
pub struct TracedRawCondvar(parking_lot::RawCondvar);

// SAFETY: Forwards to `parking_lot::RawCondvar` with the inner mutex and its
// guard state, which is the pairing that type requires.
unsafe impl lock_api::RawCondvar for TracedRawCondvar {
    const INIT: Self = Self(parking_lot::RawCondvar::INIT);

    type RawMutex = TracedRawMutex;

    unsafe fn wait(&self, mutex: &TracedRawMutex, guard: &mut TracedGuard<InnerMutexGuard>) {
        guard.release();
        // SAFETY: The caller guarantees `guard` holds `mutex`, so `guard.inner`
        // holds `mutex.inner`; parking_lot restores it before returning.
        unsafe { self.0.wait(&mutex.inner, &mut guard.inner) };
        mutex.holder.store(site_addr(guard.site), Ordering::Relaxed);
        guard.acquired = hold_start();
    }

    fn notify_one(&self) -> bool {
        self.0.notify_one()
    }

    fn notify_all(&self) -> usize {
        self.0.notify_all()
    }
}

/// Merged stats for one call site.
#[derive(Clone, Debug)]
pub struct SiteReport {
    /// Call site that acquired the lock.
    pub site: Site,
    /// Acquisition kind.
    pub kind: LockKind,
    /// Aggregated timings.
    pub stats: SiteStats,
}

/// One waiter/holder pair from contended acquisitions.
#[derive(Clone, Debug)]
pub struct BlameReport {
    /// Site that had to wait.
    pub waiter: Site,
    /// Waiter's acquisition kind.
    pub kind: LockKind,
    /// Site that last acquired the lock before the wait, if it has released
    /// the lock at least once since the last [`reset`].
    pub holder: Option<Site>,
    /// Number of contended acquisitions for this pair.
    pub count: u64,
    /// Total wait for this pair in nanoseconds.
    pub wait_ns: u64,
}

/// Snapshot of all threads' stats.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// Per-site stats, unsorted.
    pub sites: Vec<SiteReport>,
    /// Waiter/holder pairs, unsorted.
    pub blame: Vec<BlameReport>,
}

/// Merges every thread's stats.
#[must_use]
pub fn snapshot() -> Snapshot {
    let mut sites: FxHashMap<(usize, LockKind), (Site, SiteStats)> = FxHashMap::default();
    let mut blame: FxHashMap<(usize, usize, LockKind), (Site, BlameStats)> = FxHashMap::default();
    for slot in REGISTRY.lock().iter() {
        let local = slot.lock();
        for (key, (site, stats)) in &local.sites {
            sites
                .entry(*key)
                .or_insert((site, SiteStats::default()))
                .1
                .merge(stats);
        }
        for (key, (site, stats)) in &local.blame {
            let entry = &mut blame.entry(*key).or_insert((site, BlameStats::default())).1;
            entry.count += stats.count;
            entry.wait_ns += stats.wait_ns;
        }
    }

    let by_addr: FxHashMap<usize, Site> = sites
        .iter()
        .map(|(&(addr, _), &(site, _))| (addr, site))
        .collect();
    Snapshot {
        blame: blame
            .into_iter()
            .map(|((_, holder, kind), (waiter, stats))| BlameReport {
                waiter,
                kind,
                holder: by_addr.get(&holder).copied(),
                count: stats.count,
                wait_ns: stats.wait_ns,
            })
            .collect(),
        sites: sites
            .into_iter()
            .map(|((_, kind), (site, stats))| SiteReport { site, kind, stats })
            .collect(),
    }
}

/// Clears all recorded stats.
pub fn reset() {
    *STARTED_AT.lock() = Some(Instant::now());
    for slot in REGISTRY.lock().iter() {
        *slot.lock() = ThreadStats::default();
    }
}

#[expect(clippy::cast_precision_loss, reason = "display only")]
fn ms(ns: u64) -> f64 {
    ns as f64 / 1e6
}

#[expect(clippy::cast_precision_loss, reason = "display only")]
fn us(ns: u64) -> f64 {
    ns as f64 / 1e3
}

/// Renders the top `limit` rows of each table as plain text.
#[must_use]
pub fn report(limit: usize) -> String {
    let Snapshot {
        mut sites,
        mut blame,
    } = snapshot();
    let mut out = String::new();

    let total_wait: u64 = sites.iter().map(|s| s.stats.wait_ns).sum();
    let total_hold: u64 = sites.iter().map(|s| s.stats.hold_ns).sum();
    let total_acq: u64 = sites.iter().map(|s| s.stats.acquisitions).sum();
    let total_contended: u64 = sites.iter().map(|s| s.stats.contended).sum();
    let _ = writeln!(
        out,
        "lock profile: {total_acq} acquisitions, {total_contended} contended, \
         {:.1} ms waiting (summed over threads), {:.1} ms held, {} sites",
        ms(total_wait),
        ms(total_hold),
        sites.len()
    );
    let _ = writeln!(out, "mode: {:?}", mode());
    let started = *STARTED_AT.lock();
    if let Some(started) = started {
        let wall_ns = duration_ns(started);
        let threads = thread::available_parallelism().map_or(1, usize::from);
        let capacity_ns = wall_ns.saturating_mul(threads as u64);
        let _ = writeln!(
            out,
            "window: {:.2} s wall x {threads} hw threads; wait = {:.2}% of that capacity",
            ms(wall_ns) / 1e3,
            100.0 * ms(total_wait) / ms(capacity_ns.max(1)),
        );
    }

    let header = format!(
        "{:>10} {:>6} {:>10} {:>9} {:>12} {:>10} {:>10} {:>10}  {:<5} site",
        "wait ms",
        "wait%",
        "acq",
        "contended",
        "avg wait us",
        "max wt us",
        "hold ms",
        "max hld us",
        "kind"
    );
    let row = |out: &mut String, s: &SiteReport| {
        let st = &s.stats;
        let pct = |part: u64, whole: u64| {
            if whole == 0 {
                0.0
            } else {
                100.0 * ms(part) / ms(whole)
            }
        };
        let avg_wait = st.wait_ns.checked_div(st.contended).map_or(0.0, us);
        let _ = writeln!(
            out,
            "{:>10.2} {:>6.2} {:>10} {:>9} {:>12.2} {:>10.1} {:>10.2} {:>10.1}  {:<5} {}",
            ms(st.wait_ns),
            pct(st.wait_ns, total_wait),
            st.acquisitions,
            st.contended,
            avg_wait,
            us(st.max_wait_ns),
            ms(st.hold_ns),
            us(st.max_hold_ns),
            s.kind.label(),
            s.site
        );
    };

    sites.sort_unstable_by_key(|s| Reverse(s.stats.wait_ns));
    let _ = writeln!(out, "\n== top sites by total wait ==\n{header}");
    for s in sites.iter().take(limit).filter(|s| s.stats.wait_ns > 0) {
        row(&mut out, s);
    }

    if total_hold > 0 {
        sites.sort_unstable_by_key(|s| Reverse(s.stats.hold_ns));
        let _ = writeln!(out, "\n== top sites by total hold ==\n{header}");
        for s in sites.iter().take(limit) {
            row(&mut out, s);
        }
    }

    blame.sort_unstable_by_key(|b| Reverse(b.wait_ns));
    let _ = writeln!(
        out,
        "\n== top blame (waiter <- last acquirer) ==\n{:>10} {:>10}  {:<5} waiter  <-  holder",
        "wait ms", "count", "kind"
    );
    for b in blame.iter().take(limit) {
        let holder = b
            .holder
            .map_or_else(|| "<unknown>".to_owned(), ToString::to_string);
        let _ = writeln!(
            out,
            "{:>10.2} {:>10}  {:<5} {}  <-  {holder}",
            ms(b.wait_ns),
            b.count,
            b.kind.label(),
            b.waiter
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Barrier;
    use std::time::Duration;

    use super::*;
    use crate::locks::{SyncCondvar, SyncMutex};

    fn site_line(site: Site) -> u32 {
        site.line()
    }

    /// Contended waits must be keyed by the caller of `lock()`, not by
    /// `lock_api` internals, and blamed on the holder's call site.
    #[test]
    fn attributes_wait_to_caller_and_holder() {
        let mutex = Arc::new(SyncMutex::new(0u32));
        let barrier = Arc::new(Barrier::new(2));
        let holder_line = line!() + 2;
        let waiter_line = {
            let guard = mutex.lock();
            let (thread_mutex, thread_barrier) = (Arc::clone(&mutex), Arc::clone(&barrier));
            let waiter = thread::spawn(move || {
                thread_barrier.wait();
                let waiter_line = line!() + 1;
                *thread_mutex.lock() += 1;
                waiter_line
            });
            barrier.wait();
            thread::sleep(Duration::from_millis(20));
            drop(guard);
            waiter.join().expect("waiter thread panicked")
        };

        let snap = snapshot();
        let blame = snap
            .blame
            .iter()
            .find(|b| b.waiter.file() == file!() && site_line(b.waiter) == waiter_line)
            .expect("waiter site recorded");
        assert!(blame.wait_ns >= 10_000_000, "wait {} ns", blame.wait_ns);
        let blamed = blame.holder.expect("holder site resolved");
        assert_eq!((blamed.file(), site_line(blamed)), (file!(), holder_line));
    }

    /// A condvar wait releases the lock, so it must not count as hold time.
    #[test]
    fn condvar_wait_is_not_hold_time() {
        let pair = Arc::new((SyncMutex::new(false), SyncCondvar::new()));
        let notifier = {
            let pair = Arc::clone(&pair);
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(30));
                *pair.0.lock() = true;
                pair.1.notify_all();
            })
        };
        let line = line!() + 1;
        let mut ready = pair.0.lock();
        while !*ready {
            pair.1.wait(&mut ready);
        }
        drop(ready);
        notifier.join().expect("notifier thread panicked");

        let snap = snapshot();
        let row = snap
            .sites
            .iter()
            .find(|s| s.site.file() == file!() && site_line(s.site) == line)
            .expect("site recorded");
        assert!(row.stats.acquisitions >= 2);
        assert!(
            row.stats.max_hold_ns < 25_000_000,
            "hold {} ns",
            row.stats.max_hold_ns
        );
    }
}
