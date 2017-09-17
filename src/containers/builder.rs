use super::transient::Transient;
use super::reflect::{DefaultInstancer, Reflect};
use resolve::{self, ResolveRoot};
use internal::{AlwaysUnique, Instancer, Node, PerRequest, Singleton};
use std::cell::Cell;
use std::marker::Unsize;
use std::marker::PhantomData;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    Singleton,
    PerRequest,
    AlwaysUnique,
}

pub struct RepositoryBuilder<C, T>
    where T: ?Sized
{
    cont: C,
    name: String,
    _p: PhantomData<fn(T)>,
}

impl<C, T> RepositoryBuilder<C, T>
    where T: ?Sized
{
    fn new(cont: C, name: String) -> Self {
        Self {
            cont,
            name,
            _p: PhantomData,
        }
    }

    pub fn exit(self) -> Builder<C> { Builder::new(self.cont) }
}

impl<C, T> RepositoryBuilder<C, T>
    where C: AsRef<Arc<Node>>, T: Send + Sync + ?Sized + 'static
{
    fn set_instancer(self, name: &String, insert: Arc<Instancer<Object = T>>) -> Self {
        let insert = Cell::new(Some(insert));
        self.cont
            .as_ref()
            .upsert_instancer(&self.name,
                              &name,
                              || insert.replace(None).unwrap(),
                              |inst| *inst = insert.replace(None).unwrap());
        self
    }
    /// TODO Maybe take `f: Box<Fn(Transient) -> ...>`?
    pub fn set_instancer_fn<F>(self, name: &String, f: F, life: Lifecycle) -> Self
        where F: Fn(Transient) -> resolve::Result<Box<T>> + Send + Sync + 'static
    {
        let create_fn =
            move |_: &_, _: &_, node: &Arc<Node>| f(Transient::rewrap(node.clone()));

        let inst: Arc<Instancer<Object = T>> = match life {
            Lifecycle::Singleton => Arc::new(Singleton::new(create_fn)),
            Lifecycle::PerRequest => Arc::new(PerRequest::new(create_fn)),
            Lifecycle::AlwaysUnique => Arc::new(AlwaysUnique::new(create_fn)),
        };

        self.set_instancer(name, inst)
    }
    pub fn set_fn<I, F>(self, f: F, life: Lifecycle) -> Self
        where I: Reflect, F: Fn(Transient) -> resolve::Result<Box<T>> + Send + Sync + 'static
    {
        self.set_instancer_fn(I::name(), f, life)
    }
    pub fn default_fn<F>(self, f: F, life: Lifecycle) -> Self
        where F: Fn(Transient) -> resolve::Result<Box<T>> + Send + Sync + 'static
    {
        self.set_fn::<DefaultInstancer, _>(f, life)
    }

    pub fn set<I, R>(self, life: Lifecycle) -> Self
        where I: Reflect, Transient: ResolveRoot<R>, R: Unsize<T>
    {
        self.set_fn::<I, _>(|t| t.resolve::<R>().map(|o| Box::new(o) as Box<T>), life)
    }
    pub fn default<R>(self, life: Lifecycle) -> Self
        where Transient: ResolveRoot<R>, R: Unsize<T>
    {
        self.set::<DefaultInstancer, R>(life)
    }

    pub fn set_instancer_object(self, name: &String, obj: Box<T>) -> Self {
        self.set_instancer(name, Arc::new(Singleton::object(obj)))
    }
    pub fn set_object<I>(self, obj: Box<T>) -> Self
        where I: Reflect
    {
        self.set_instancer_object(I::name(), obj)
    }
    pub fn default_object(self, obj: Box<T>) -> Self {
        self.set_object::<DefaultInstancer>(obj)
    }
}

pub struct Builder<C> {
    cont: C,
}

impl<C> Builder<C> {
    pub fn new(cont: C) -> Self { Self { cont } }

    pub fn repository_builder<T>(self, name: &str) -> RepositoryBuilder<C, T>
        where T: ?Sized
    {
        RepositoryBuilder::new(self.cont, name.to_owned())
    }
    pub fn repository<T>(self) -> RepositoryBuilder<C, T>
        where T: Reflect + ?Sized
    {
        self.repository_builder(T::name())
    }

    pub fn build(self) -> C { self.cont }
}
