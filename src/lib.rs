use std::ops::{Deref, DerefMut};
use std::sync::Arc;

/// This crate provides type FinArc, which is Arc with finalizer callback, that
/// clones inner data on cloning and calls finalizer when last instance is dropped
///
/// It may be useful in situations where you have internally synchronized type that is `Clone`able,
/// but all its clones belongs to single resource and that resource must be freed when last clone is dropped
///
/// One such example is `Channel` in `lapin` crate, that belongs to RabbitMQ channel.
/// It is cloneable, but it doesn't follow RAII, so, no close on the drop
/// and any thread that uses one copy of channel may close it, leaving other threads to face
/// rabbit errors when they try to use this already closed channel.
///
/// Suppose you want to create your own wrapper around this rather unsafe type.
/// You may use `Arc<Mutex/RwLock<Channel>>`, but it is inefficient, as Channel is already synchronized
/// (it is simply bunch of `Arc<Mutex/RwLock>`s), so you need to clone them, but in the same time, track
/// the number of copies to call `.close()` on last instance before drop.
/// FinArc allows you to do that, you just provides `FnOnce(&mut Channel)` callback, that is called
/// when last channel copy is dropped.
///
/// Unlike `Arc`, `FinArc<T, F>` implements `DerefMut` to `T`, because each instance of `FinArc` owns
/// its own copy of `T`
pub struct FinArc<T, F>
where
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
}

impl<T, F> Deref for FinArc<T, F>
where
    F: FnOnce(&mut T),
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T, F> DerefMut for FinArc<T, F>
where
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
}
