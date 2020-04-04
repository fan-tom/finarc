[![API Docs][Doc Version]][Doc]
[![Downloads][Downloads]][Crate]

# finarc

This crate provides type FinArc, which is Arc with finalizer callback, that
clones inner data on cloning and calls finalizer when last instance is dropped

It may be useful in situations where you have internally synchronized type that is `Clone`able,
but all its clones belongs to single resource and that resource must be released when last clone is dropped

One such example is `Channel` in `lapin` crate, that belongs to RabbitMQ channel.
It is cloneable, but it doesn't follow RAII, that is, no close on the drop
and any thread that uses one copy of channel may close it, leaving other threads to face
Rabbit errors when they try to use this already closed channel.

Suppose you want to create your own wrapper around this type.
You may use `Arc<Mutex/RwLock<Channel>>`, but it is inefficient, as Channel is already synchronized
(it is simply bunch of `Arc<Mutex/RwLock>`s), so you need to clone them, but in the same time, track
the number of copies to call `.close()` on last instance before drop.
FinArc allows you to do that, you just provide `FnOnce(&mut Channel)` callback, that is called
when last channel copy is dropped.

Unlike `Arc`, `FinArc<T, F>` implements `DerefMut` to `T`, because each instance of `FinArc` owns
its own copy of `T`

Current Version: 0.2.0

License: MIT OR Apache-2.0

[Doc Version]: https://docs.rs/finarc/badge.svg
[Doc]: https://docs.rs/finarc
[Downloads]: https://img.shields.io/crates/d/finarc.svg
[Crate]: https://crates.io/crates/finarc
