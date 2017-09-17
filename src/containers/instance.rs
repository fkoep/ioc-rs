use super::reflect::DefaultInstancer;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;

// ++++++++++++++++++++ Instance ++++++++++++++++++++

pub struct Instance<T, I = DefaultInstancer>
    where T: ?Sized
{
    inner: Arc<Box<T>>,
    _p: PhantomData<fn(I)>,
}

impl<T, I> Instance<T, I>
    where T: ?Sized
{
    pub fn new(inner: Arc<Box<T>>) -> Self {
        Self {
            inner,
            _p: PhantomData,
        }
    }
}

impl<T, I> Clone for Instance<T, I>
    where T: ?Sized
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _p: PhantomData,
        }
    }
}

impl<T, I> Deref for Instance<T, I>
    where T: ?Sized
{
    type Target = T;
    fn deref(&self) -> &Self::Target { &self.inner }
}
