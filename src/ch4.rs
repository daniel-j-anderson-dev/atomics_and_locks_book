//! Building Our Own Spin Lock Summary
//! - A spin lock is a mutex that busy-loops, or spins, while waiting.
//! - Spinning can reduce latency, but can also be a waste of clock cycles and reduce performance.
//! - A spin loop hint, std::hint::spin_loop(), can be used to inform the
//!   processor of a spin loop, which might increase its efficiency.
//! - A SpinLock<T> can be implemented with just an AtomicBool and an UnsafeCell<T>, the latter of
//!   which is necessary for interior mutability (see "Interior Mutability" in Chapter 1).
//! - A happens-before relationship between unlock and lock operations is necessary to prevent a
//!   data race, which would result in undefined behavior.
//! - Acquire and release memory ordering are a perfect fit for this use case.
//! - When making unchecked assumptions necessary to avoid undefined behavior, the responsibility
//!   can be shifted to the caller by making the function unsafe.
//! - The Deref and DerefMut traits can be used to make a type behave like a reference, transparently
//!   providing access to another object.
//! - The Drop trait can be used to do something when an object is dropped, such as when it goes out
//!   of scope, or when it is passed to drop().
//! - A lock guard is a useful design pattern of a special type that’s used to represent (safe) access
//!   to a locked lock. Such a type usually behaves similarly to a reference, thanks to the Deref traits,
//!   and implements automatic unlocking through the Drop trait.

use super::*;

/// This struct is a small wrapper around [AtomicBool] representing whether some arbitrary data is accessible (**unlocked**).
/// - use [SpinLockFlag::lock] to signal any other threads that some data is locked and should not be accessed.
/// - use [SpinLockFlag::unlock] to signal any other threads that some data is unlocked and another thread can lock.
/// ## Safety
/// The caller needs to make sure that any static mut data is only accessed while the [SpinLockFlag] instance is locked
pub struct SpinLockFlag {
    is_locked: AtomicBool,
}
impl SpinLockFlag {
    pub const fn new() -> Self {
        return Self {
            is_locked: AtomicBool::new(false),
        };
    }
    pub fn lock(&self) {
        // TODO: after a set number of loops we should put this thread to sleep before spinning again
        while self
            .is_locked
            // if is_locked == false, then acquire-load the old_value to be returned; afterwards relaxed-store true value to is_locked. return old_value as an Ok
            // else relaxed-load the old_value and return it as an Err
            .compare_exchange_weak(false, true, Acquire, Relaxed)
            .is_err()
        {
            // tell the OS that we are waiting using a loop.
            // OS doesn't have to listen
            std::hint::spin_loop();
        }
    }
    pub fn unlock(&self) {
        self.is_locked.store(false, Release);
    }
}

/// This spin lick is similar to [SpinLockFlag] except the protected data is managed by this type using a [UnsafeCell].
/// [UnsafeSpinLock] implements [Sync] for types that are [Send] because only one reference to the inner `T` is given out.
pub struct UnsafeSpinLock<T> {
    protector: SpinLockFlag,
    value: UnsafeCell<T>,
}
// Note that we don’t need to require that T is Sync, because our SpinLock<T> will only allow one thread at a time to access the T it protects.
// Only if we were to give multiple threads access at once, like a reader-writer lock does for readers, would we (additionally) need to require T: Sync.
unsafe impl<T: Send> Sync for UnsafeSpinLock<T> {}
impl<T> UnsafeSpinLock<T> {
    pub const fn new(value: T) -> Self {
        return Self {
            protector: SpinLockFlag::new(),
            value: UnsafeCell::new(value),
        };
    }
    pub fn lock<'a>(&'a self) -> &'a mut T {
        self.protector.lock();
        let pointer = self.value.get();
        return unsafe { &mut *pointer };
    }
    /// # Safety
    /// The mutable reference from [UnsafeSpinLock::lock] must be gone!!
    /// This includes any references to fields of `T`
    pub unsafe fn unlock(&self) {
        self.protector.unlock();
    }
}

mod safe_spin_lock {
    use std::ops::{Deref, DerefMut};

    use super::*;

    /// Identical to [UnsafeSpinLock] except that [SpinLock::lock] returns a [Guard<'a, T>] not a `&mut T`
    pub struct SpinLock<T> {
        protector: SpinLockFlag,
        value: UnsafeCell<T>,
    }
    unsafe impl<T: Send> Sync for SpinLock<T> {}
    impl<T> SpinLock<T> {
        pub const fn new(value: T) -> Self {
            return Self {
                protector: SpinLockFlag::new(),
                value: UnsafeCell::new(value),
            };
        }
        pub fn lock<'a>(&'a self) -> Guard<'a, T> {
            self.protector.lock();
            return Guard { guarded: self };
        }
    }

    /// [Guard] cant outlive it's [SpinLock].
    /// A [Guard] value can only be acquired by calling [SpinLock::lock].
    /// Because
    ///   - [Guard] has no constructors
    ///   - Struct initializer syntax CANNOT be used because
    ///     - guard's field is private
    ///     - [Guard] is defined in a unique module
    ///
    /// [Guard] is [Deref] as `T` and [DerefMut] as `T`
    pub struct Guard<'a, T> {
        guarded: &'a SpinLock<T>,
    }
    unsafe impl<T: Sync> Sync for Guard<'_, T> {}
    impl<T> Drop for Guard<'_, T> {
        fn drop(&mut self) {
            self.guarded.protector.unlock();
        }
    }
    impl<T> Deref for Guard<'_, T> {
        type Target = T;
        fn deref(&self) -> &Self::Target {
            // SAFETY: Guard's invariant is that it only exists
            // when there is exclusive access to the inner T.
            // self.guarded.protector.is_locked == true
            return unsafe { &*self.guarded.value.get() };
        }
    }
    impl<T> DerefMut for Guard<'_, T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            // SAFETY: Guard's invariant is that it only exists
            // when there is exclusive access to the inner T.
            // self.guarded.protector.is_locked == true
            return unsafe { &mut *self.guarded.value.get() };
        }
    }
}
use safe_spin_lock::*;

#[test]
fn safe_spin_lock() {
    static DATA: SpinLock<Vec<usize>> = SpinLock::new(Vec::new());

    thread::scope(|s| {
        for i in 0..10 {
            s.spawn(move || {
                DATA.lock().push(i);
                thread::sleep(Duration::from_secs(1));
            });
        }
        for i in 10..20 {
            s.spawn(move || {
                DATA.lock().push(i);
            });
        }
    });

    for i in DATA.lock().iter() {
        print!("{}, ", i);
    }
}

#[test]
fn poison_spin_lock() {
    static DATA: SpinLock<Vec<usize>> = SpinLock::new(Vec::new());

    thread::spawn(move || {
        let data = DATA.lock();
        panic!("uh oh the guard is never dropped!");
    });

    thread::sleep(Duration::from_secs(3));

    for i in 0..10 {
        thread::spawn(move || {
            DATA.lock().push(i);
        });
    }

    println!("Data:");
    for i in DATA.lock().iter() {
        print!("{}, ", i);
    }
}
