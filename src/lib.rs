#![deny(broken_intra_doc_links, rust_2018_idioms)]
#![warn(
    missing_copy_implementations,
    missing_debug_implementations,
    clippy::explicit_iter_loop,
    clippy::future_not_send,
    clippy::use_self,
    clippy::clone_on_ref_ptr
)]

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{OwnedRwLockReadGuard, RwLock};
use tokio::task::JoinHandle;

/// Like a [`Arc`][arc] but invokes [`async_drop`][async_drop] when the last `Arcy` pointer
/// is destroyed.
///
/// The type `Arcy<T>` provides shared ownership of a value of type `T`, allocated in the heap.
/// Invoking [`clone`][clone] on `Arcy` produces a new Arc instance, which points to the same allocation
/// on the heap as the source `Arcy`, while increasing a reference count.
/// When the last `Arcy` pointer to a given allocation is destroyed, [`AsyncDrop::async_drop`]
/// on the value stored in that allocation (often referred to as “inner value”),
/// and after the completion of that async function, the value is dropped  
///
/// Shared references in Rust disallow mutation by default, and `Arcy` is no exception:
/// you cannot generally obtain a mutable reference to something inside an `Arcy`.
/// If you need to mutate through an `Arcy`, [`Mutex`][mutex], [`RwLock`][rwlock], or one of the [`Atomic`][atomic]
///
/// ## `Deref` behavior
///
/// `Arc<T>` automatically dereferences to `T` (via the [`Deref`][deref] trait),
/// so you can call `T`'s methods on a value of type `Arcy<T>`. To avoid name
/// clashes with `T`'s methods, the methods of `Arcy<T>` itself are associated
/// functions, called using [fully qualified syntax].
///
/// `Arcy<T>`'s implementations of [`clone`][clone] is an associated async function.
///
/// # Examples
///
/// ```
/// use arcy::{Arcy, AsyncDrop};
///
/// struct Foo {}
///
/// #[async_trait::async_trait]
/// impl AsyncDrop for Foo {
///     async fn async_drop(&self) {
///         // do something asynchronously
///     }
/// }
///
/// async fn do_something(_foo: Arcy<Foo>) {
///     // ...
/// }
///
/// #[tokio::main]
/// async fn main() {
///     let (foo, last_foo) = Arcy::new(Foo {}).await;
///     let j1 = tokio::spawn(do_something(Arcy::clone(&foo).await));
///     let j2 = tokio::spawn(do_something(foo));
///
///     tokio::try_join!(j1, j2).unwrap();
///
///     last_foo.await.unwrap();
/// }
/// ```
///
/// [arc]: std::sync::Arc
/// [clone]: Clone::clone
/// [async_drop]: AsyncDrop::async_drop
/// [mutex]: std::sync::Mutex
/// [rwlock]: std::sync::RwLock
/// [atomic]: core::sync::atomic
/// [deref]: core::ops::Deref
/// [fully qualified syntax]: https://doc.rust-lang.org/book/ch19-03-advanced-traits.html#fully-qualified-syntax-for-disambiguation-calling-methods-with-the-same-name
#[derive(Debug)]
pub struct Arcy<T>
where
    T: AsyncDrop,
{
    inner: Arc<RwLock<T>>,
    pub read: OwnedRwLockReadGuard<T>,
}

/// Called when an [`Arcy`] is destroyed.
#[async_trait]
pub trait AsyncDrop {
    async fn async_drop(&self);
}

impl<T> Arcy<T>
where
    T: AsyncDrop + Send + Sync + 'static,
{
    /// Constructs a new `Arcy<T>`.
    pub async fn new(inner: T) -> (Self, JoinHandle<()>) {
        let inner = Arc::new(RwLock::new(inner));
        let read = Arc::clone(&inner).read_owned().await;
        let slayer = tokio::spawn(Self::slayer(Arc::clone(&inner)));
        (Self { inner, read }, slayer)
    }

    pub async fn clone(this: &Self) -> Self {
        let inner = Arc::clone(&this.inner);
        let read = inner.read_owned().await;
        let inner = Arc::clone(&this.inner);
        Self { inner, read }
    }

    pub async fn slayer(inner: Arc<RwLock<T>>) {
        let inner = inner.write().await;
        // TODO select some timer and log if we're waiting too long
        inner.async_drop().await;
    }
}

impl<T> std::ops::Deref for Arcy<T>
where
    T: AsyncDrop,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.read.deref()
    }
}
