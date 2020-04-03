[![API Docs](https://docs.rs/finarc/badge.svg)](https://docs.rs/finarc)
[![Downloads](https://img.shields.io/crates/d/finarc.svg)](https://crates.io/crates/finarc)

This crate provides type FinArc, which is Arc with finalizer callback, that
clones inner data on cloning and calls finalizer when last instance is dropped

It may be useful in situations where you have internally synchronized type that is `Clone`able,
but all its clones belongs to single resource and that resource must be freed when last clone is dropped

One such example is [`Channel`](https://docs.rs/lapin/0.36.2/lapin/struct.Channel.html) in [`lapin`](https://docs.rs/lapin) crate, that belongs to RabbitMQ channel.
It is cloneable, but it doesn't follow RAII, so, no close on the drop
and any thread that uses one copy of channel may close it, leaving other threads to face
rabbit errors when they try to use this already closed channel.

Suppose you want to create your own wrapper around this type.
You may use `Arc<Mutex/RwLock<Channel>>`, but it is inefficient, as Channel is already synchronized
(it is simply bunch of `Arc<Mutex/RwLock>`s), so you need to clone them, but in the same time, track
the number of copies to call `.close()` on last instance before drop.
FinArc allows you to do that, you just provides `FnOnce(&mut Channel)` callback, that is called
when last channel copy is dropped.

Unlike `Arc`, `FinArc<T, F>` implements `DerefMut` to `T`, because each instance of `FinArc` owns
its own copy of `T`
