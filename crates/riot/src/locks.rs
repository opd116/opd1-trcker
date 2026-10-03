//! Poison-tolerant locking for this crate's `std::sync` locks.
//!
//! A panic while a lock is held poisons it, and `lock().unwrap()` would then
//! panic in every later call: one bug in one caller would take the whole
//! session down with it (the throttle, the auth state, the asset tables). None
//! of the data behind these locks is left half-written by a panic (counters,
//! caches, maps, a token), so the guard is taken anyway — the same helper the
//! app crate keeps in `state.rs`.

use std::sync::{Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// `m.lock()`, with a poisoned lock taken as it is.
pub(crate) fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `l.read()`, with a poisoned lock taken as it is.
pub(crate) fn read_lock<T>(l: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    l.read().unwrap_or_else(PoisonError::into_inner)
}

/// `l.write()`, with a poisoned lock taken as it is.
pub(crate) fn write_lock<T>(l: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    l.write().unwrap_or_else(PoisonError::into_inner)
}
