//! This crate provides type FinArc, which is Arc with finalizer callback, that
//! clones inner data on cloning and calls finalizer when last instance is dropped
//!
//! It may be useful in situations where you have internally synchronized type that is `Clone`able,
//! but all its clones belongs to single resource and that resource must be released when last clone is dropped
//!
//! One such example is `Channel` in `lapin` crate, that belongs to RabbitMQ channel.
//! It is cloneable, but it doesn't follow RAII, that is, no close on the drop
//! and any thread that uses one copy of channel may close it, leaving other threads to face
//! Rabbit errors when they try to use this already closed channel.
//!
//! Suppose you want to create your own wrapper around this type.
//! You may use `Arc<Mutex/RwLock<Channel>>`, but it is inefficient, as Channel is already synchronized
//! (it is simply bunch of `Arc<Mutex/RwLock>`s), so you need to clone them, but in the same time, track
//! the number of copies to call `.close()` on last instance before drop.
//! FinArc allows you to do that, you just provide `FnOnce(&mut Channel)` callback, that is called
//! when last channel copy is dropped.
//!
//! Unlike `Arc`, `FinArc<T, F>` implements `DerefMut` to `T`, because each instance of `FinArc` owns
//! its own copy of `T`

use std::cmp::Ordering;
use std::fmt;
use std::mem;
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::sync::Arc;

pub struct FinArc<T, F>
where
    T: ?Sized,
    F: FnOnce(&mut T),
{
    // We will use this field to both don't clone finalizer and to detect when last instance is dropped
    // Option here as FnOnce accepts `self` by value, and we can take Arc to try to get finalizer if it is possible
    // Arc<Option<T>> has smaller footprint than Option<Arc<T>> if T can be all-zeros, but FnOnce is not that case
    inner: Option<Arc<F>>,
    data: T,
}

impl<T, F> FinArc<T, F>
where
    F: FnOnce(&mut T),
{
    pub fn new(data: T, finalizer: F) -> Self {
        Self {
            inner: Some(Arc::new(finalizer)),
            data,
        }
    }

    /// Returns the contained value, if this is the last instance of FinArc, without running finalizer
    ///
    /// Otherwise, an [`Err`][result] is returned with the same `FinArc` that was
    /// passed in.
    ///
    /// [result]: ../../std/result/enum.Result.html
    ///
    /// # Examples
    ///
    /// ```
    /// use finarc::FinArc;
    ///
    /// let x = FinArc::new(3, |_|{});
    /// assert_eq!(FinArc::try_unwrap(x), Ok(3));
    ///
    /// let x = FinArc::new(4, |_|{});
    /// let _y = FinArc::clone(&x);
    /// assert_eq!(*FinArc::try_unwrap(x).unwrap_err(), 4);
    /// ```
    /// Analogue of `Arc::try_unwrap`
    pub fn try_unwrap(mut this: Self) -> Result<T, Self> {
        match Arc::try_unwrap(this.inner.take().expect("Finalizer is gone")) {
            Ok(_) => unsafe {
                // we cannot simply move out of FinArc, because it has custom impl of Drop
                let data = ptr::read(&this.data);
                // avoid calling Drop, we already dropped finalizer and "moved" data
                mem::forget(this);
                Ok(data)
            },
            Err(arc) => {
                this.inner = Some(arc);
                Err(this)
            }
        }
    }
}

impl<T, F> Deref for FinArc<T, F>
where
    T: ?Sized,
    F: FnOnce(&mut T),
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T, F> DerefMut for FinArc<T, F>
where
    T: ?Sized,
    F: FnOnce(&mut T),
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl<T, F> Clone for FinArc<T, F>
where
    T: Clone,
    F: FnOnce(&mut T),
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            data: self.data.clone(),
        }
    }
}

impl<T, F> Drop for FinArc<T, F>
where
    T: ?Sized,
    F: FnOnce(&mut T),
{
    fn drop(&mut self) {
        // Here we both checked that it is the last instance and got callback from it, double win!
        // If it is not last instance, Err will return Arc back and it will be dropped normally, without calling finalizer
        if let Ok(f) = Arc::try_unwrap(self.inner.take().expect("Finalizer is gone")) {
            (f)(&mut self.data);
        }
    }
}

/// We ignore finalizers when comparing FinArc's, so they may be of different types
impl<T, F, F1> PartialEq<FinArc<T, F1>> for FinArc<T, F>
where
    T: ?Sized + PartialEq,
    F: FnOnce(&mut T),
    F1: FnOnce(&mut T),
{
    /// Equality for two `FinArc`s.
    ///
    /// Two `FinArc`s are equal if their inner values are equal.
    ///
    /// If `T` also implements `Eq`, two `FinArc`s that point to the same value are
    /// always equal.
    ///
    /// # Examples
    ///
    /// ```
    /// use finarc::FinArc;
    ///
    /// let five = FinArc::new(5, |_|{});
    ///
    /// assert!(five == FinArc::new(5, |_|{}));
    /// ```
    #[inline]
    fn eq(&self, other: &FinArc<T, F1>) -> bool {
        (**self).eq(&**other)
    }

    /// Inequality for two `FinArc`s.
    ///
    /// Two `FinArc`s are unequal if their inner values are unequal.
    ///
    /// If `T` also implements `Eq`, two `FinArc`s that point to the same value are
    /// never unequal.
    ///
    /// # Examples
    ///
    /// ```
    /// use finarc::FinArc;
    ///
    /// let five = FinArc::new(5, |_|{});
    ///
    /// assert!(five != FinArc::new(6, |_|{}));
    /// ```
    #[inline]
    fn ne(&self, other: &FinArc<T, F1>) -> bool {
        (**self).ne(&**other)
    }
}

/// We ignore finalizers when comparing FinArc's, so they may be of different types
impl<T, F, F1> PartialOrd<FinArc<T, F1>> for FinArc<T, F>
where
    T: ?Sized + PartialOrd,
    F: FnOnce(&mut T),
    F1: FnOnce(&mut T),
{
    /// Partial comparison for two `FinArc`s.
    ///
    /// The two are compared by calling `partial_cmp()` on their inner values.
    ///
    /// # Examples
    ///
    /// ```
    /// use finarc::FinArc;
    /// use std::cmp::Ordering;
    ///
    /// let five = FinArc::new(5, |_|{});
    ///
    /// assert_eq!(Some(Ordering::Less), five.partial_cmp(&FinArc::new(6, |_|{})));
    /// ```
    fn partial_cmp(&self, other: &FinArc<T, F1>) -> Option<Ordering> {
        (**self).partial_cmp(&**other)
    }

    /// Less-than comparison for two `FinArc`s.
    ///
    /// The two are compared by calling `<` on their inner values.
    ///
    /// # Examples
    ///
    /// ```
    /// use finarc::FinArc;
    ///
    /// let five = FinArc::new(5, |_|{});
    ///
    /// assert!(five < FinArc::new(6, |_|{}));
    /// ```
    fn lt(&self, other: &FinArc<T, F1>) -> bool {
        *(*self) < *(*other)
    }

    /// 'Less than or equal to' comparison for two `FinArc`s.
    ///
    /// The two are compared by calling `<=` on their inner values.
    ///
    /// # Examples
    ///
    /// ```
    /// use finarc::FinArc;
    ///
    /// let five = FinArc::new(5, |_|{});
    ///
    /// assert!(five <= FinArc::new(5, |_|{}));
    /// ```
    fn le(&self, other: &FinArc<T, F1>) -> bool {
        *(*self) <= *(*other)
    }

    /// Greater-than comparison for two `FinArc`s.
    ///
    /// The two are compared by calling `>` on their inner values.
    ///
    /// # Examples
    ///
    /// ```
    /// use finarc::FinArc;
    ///
    /// let five = FinArc::new(5, |_|{});
    ///
    /// assert!(five > FinArc::new(4, |_|{}));
    /// ```
    fn gt(&self, other: &FinArc<T, F1>) -> bool {
        *(*self) > *(*other)
    }

    /// 'Greater than or equal to' comparison for two `FinArc`s.
    ///
    /// The two are compared by calling `>=` on their inner values.
    ///
    /// # Examples
    ///
    /// ```
    /// use finarc::FinArc;
    ///
    /// let five = FinArc::new(5, |_|{});
    ///
    /// assert!(five >= FinArc::new(5, |_|{}));
    /// ```
    fn ge(&self, other: &FinArc<T, F1>) -> bool {
        *(*self) >= *(*other)
    }
}

impl<T: ?Sized + Ord, F: FnOnce(&mut T)> Ord for FinArc<T, F> {
    /// Comparison for two `FinArc`s.
    ///
    /// The two are compared by calling `cmp()` on their inner values.
    ///
    /// # Examples
    ///
    /// ```
    /// use finarc::FinArc;
    /// use std::cmp::Ordering;
    ///
    /// let five = FinArc::new(5, |_|{});
    /// let mut six = FinArc::clone(&five);
    /// *six = 6;
    ///
    /// assert_eq!(Ordering::Less, five.cmp(&six));
    /// ```
    fn cmp(&self, other: &FinArc<T, F>) -> Ordering {
        (**self).cmp(&**other)
    }
}

impl<T: ?Sized + Eq, F: FnOnce(&mut T)> Eq for FinArc<T, F> {}

impl<T: ?Sized + fmt::Display, F: FnOnce(&mut T)> fmt::Display for FinArc<T, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&**self, f)
    }
}

impl<T: ?Sized + fmt::Debug, F: FnOnce(&mut T)> fmt::Debug for FinArc<T, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}

#[cfg(test)]
mod test {
    use super::FinArc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_finalizer_is_called_once_last_clone_is_dropped() {
        #[derive(Clone)]
        struct ManuallyClosable<'a>(&'a AtomicUsize);
        impl ManuallyClosable<'_> {
            fn close(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let close_counter = AtomicUsize::new(0);
        let data = ManuallyClosable(&close_counter);
        let arc = FinArc::new(data, |data| data.close());
        let arc_clone = arc.clone();
        drop(arc);
        assert_eq!(close_counter.load(Ordering::SeqCst), 0);
        drop(arc_clone);
        assert_eq!(close_counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn try_unwrap_doesnt_call_drop() {
        #[derive(Clone)]
        struct ManuallyClosable<'a>(&'a AtomicUsize);
        impl ManuallyClosable<'_> {
            fn close(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let close_counter = AtomicUsize::new(0);
        let data = ManuallyClosable(&close_counter);
        let arc = FinArc::new(data, |data| data.close());
        assert!(FinArc::try_unwrap(arc).is_ok());
        assert_eq!(close_counter.load(Ordering::SeqCst), 0);

        let data = ManuallyClosable(&close_counter);
        let arc = FinArc::new(data, |data| data.close());
        let arc_clone = arc.clone();
        assert!(FinArc::try_unwrap(arc).is_err());
        assert_eq!(close_counter.load(Ordering::SeqCst), 0);
        drop(arc_clone);
        assert_eq!(close_counter.load(Ordering::SeqCst), 1);
    }
}
