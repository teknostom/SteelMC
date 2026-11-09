#![expect(
    clippy::disallowed_types,
    reason = "this module is the canonical definition of the allowed lock types"
)]
//! Lock wrappers for debug checks and deadlock prevention.
//!
//! With the `lock-profiling` feature, the synchronous locks record per-call-site
//! wait and hold times; see [`profiling`].

use std::sync::Arc;

use parking_lot::lock_api;
use tokio::sync::{Mutex, RwLock};

#[cfg(feature = "lock-profiling")]
pub mod profiling;

#[cfg(not(feature = "lock-profiling"))]
type RawMutex = parking_lot::RawMutex;
#[cfg(not(feature = "lock-profiling"))]
type RawRwLock = parking_lot::RawRwLock;
#[cfg(not(feature = "lock-profiling"))]
type RawCondvar = parking_lot::RawCondvar;

#[cfg(feature = "lock-profiling")]
type RawMutex = profiling::TracedRawMutex;
#[cfg(feature = "lock-profiling")]
type RawRwLock = profiling::TracedRawRwLock;
#[cfg(feature = "lock-profiling")]
type RawCondvar = profiling::TracedRawCondvar;

/// A synchronous mutex.
pub type SyncMutex<T> = lock_api::Mutex<RawMutex, T>;
/// Guard returned by [`SyncMutex::lock`].
pub type SyncMutexGuard<'a, T> = lock_api::MutexGuard<'a, RawMutex, T>;
/// Owned guard returned by [`SyncMutex::lock_arc`].
pub type SyncArcMutexGuard<T> = lock_api::ArcMutexGuard<RawMutex, T>;
/// A synchronous read-write lock.
pub type SyncRwLock<T> = lock_api::RwLock<RawRwLock, T>;
/// Guard returned by [`SyncRwLock::read`].
pub type SyncRwLockReadGuard<'a, T> = lock_api::RwLockReadGuard<'a, RawRwLock, T>;
/// Guard returned by [`SyncRwLock::write`].
pub type SyncRwLockWriteGuard<'a, T> = lock_api::RwLockWriteGuard<'a, RawRwLock, T>;
/// A condition variable for [`SyncMutex`].
pub type SyncCondvar = lock_api::Condvar<RawCondvar>;

/// An asynchronous mutex.
pub type AsyncMutex<T> = Mutex<T>;
/// An asynchronous read-write lock.
pub type AsyncRwLock<T> = RwLock<T>;

/// A value shared across threads behind the crate's standard lock.
pub type Shared<T> = Arc<SyncMutex<T>>;

/// Wraps `value` in the standard shared lock handle.
pub fn shared<T>(value: T) -> Shared<T> {
    Arc::new(SyncMutex::new(value))
}

/// Wraps this value in an `Arc<SyncMutex<>>`
pub trait IntoShared: Sized {
    /// Wraps this value in an `Arc<SyncMutex<>>`
    fn into_shared(self) -> Shared<Self> {
        Arc::new(SyncMutex::new(self))
    }
}

impl<T> IntoShared for T {}
