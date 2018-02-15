use reflect::Reflect;
use resolve::*;
use middleware::*;
use downcast::Downcast;
use std::marker::{PhantomData, Unsize};
use std::sync::Arc;
use std::ops::{Deref};
use std::result::Result as StdResult;
use std::error::Error as StdError;

pub struct Main(());

impl Reflect for Main {
    fn name_init() -> String { "main".to_owned() }
}

pub struct Return(());

impl Reflect for Return {
    fn name_init() -> String { "return".to_owned() }
}

fn cache_name_to_opt(name: &str) -> Option<String> {
    if name == Return::name() {
        None
    } else {
        Some(name.to_owned())
    }
}

// ++++++++++++++++++++ Composition ++++++++++++++++++++

// TODO move this somewhere else?
// pub trait Create<T>: Send + Sync + 'static {
//     type Depend;
//     type Error: Error + Send + Sync;
//     fn create(&self, depend: Self::Depend) -> Result<T, Self::Error>;
// }

// TODO move this somewhere else?
impl RedirectRules {
    pub fn service<F, T>(mut self)  -> Self 
        where F: Reflect + ?Sized, T: Reflect + ?Sized
    {
        self.service.insert(F::name().to_owned(), T::name().to_owned());
        self
    }
    pub fn alternative<F, T>(mut self)  -> Self 
        where F: Reflect, T: Reflect
    {
        self.alternative.insert(F::name().to_owned(), T::name().to_owned());
        self
    }
    pub fn for_cache<F, T>(mut self)  -> Self 
        where F: Reflect, T: Reflect
    {
        self.for_cache.insert(cache_name_to_opt(F::name()), cache_name_to_opt(T::name()));
        self
    }
}

#[derive(Clone)]
pub struct Composition(Arc<Middleware>);

impl Container for Composition {
    type Error = Error;
}

impl ResolveStart<Composition> for Composition {
    fn resolve_start(&self) -> Result<Self> { Ok(self.clone()) }
}

impl<R> ResolveStart<Option<R>> for Composition 
    where R: Resolve, Self: ResolveStart<R>
{
    fn resolve_start(&self) -> Result<Option<R>> {
        match <Self as ResolveStart<R>>::resolve_start(&self) {
            Ok(r) => Ok(Some(r)),
            Err(Error::InstanceNotFound{ .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

// TODO reduce unneccesary .clone()'s.
impl Composition {
    pub fn new(mw: Arc<Middleware>) -> Self { Composition(mw) }
    
    pub fn instantiate(&self, svc: String, alt: String) -> Result<InstanceHandle> {
        let req = Request::new(self.0.clone(), svc, alt);
        let resp = self.0.instantiate(req)?;
        if let Some(for_cache) = resp.for_cache {
            /* TODO return error? */
            assert_eq!(for_cache, Return::name(), "Missing cache '{}'", for_cache);
        }
        Ok(resp.handle)
    }
    
    pub fn resolve<R>(&self) -> Result<R>
        where Composition: ResolveStart<R>
    {
        self.resolve_start()
    }

    pub fn root() -> Self { Self::new(Arc::new(Root)) }

    pub fn map<M, F>(self, f: F) -> Self
        where M: Middleware + 'static, F: FnOnce(Arc<Middleware>) -> M
    {
        Self::new(Arc::new(f(self.0)))
    }

    pub fn with_alternative_fn<Svc, Alt, Ca, Impl, E, F>(self, f: F) -> Self
    where 
        Svc: Reflect + Send + Sync + ?Sized, 
        Alt: Reflect,
        Ca: Reflect, 
        Impl: Resolve + Unsize<Svc>,
        E: StdError + Send + Sync + 'static,
        F: Fn(&Self) -> StdResult<Impl, E> + Send + Sync + 'static
    {
        let svc = Svc::name().to_owned();
        let alt = Alt::name().to_owned();
        let create_fn = box move |mw| {
            f(&Composition::new(mw))
                .map(|imp| box {box imp as Box<Svc>} as InstanceObject)
                .map_err(|e| box e as GenericError)
        };
        let for_cache = cache_name_to_opt(Ca::name());
        self.map(|this| WithFactory::new(this, svc, alt, create_fn, for_cache))
    }

    pub fn with_alternative<Svc, Alt, Ca, Impl>(self) -> Self
    where 
        Svc: Reflect + Send + Sync + ?Sized, 
        Alt: Reflect,
        Ca: Reflect, 
        Impl: Resolve + Unsize<Svc>,
        Self: ResolveStart<Impl>,
    {
        self.with_alternative_fn::<Svc, Alt, Ca, Impl, _, _>(Self::resolve::<Impl>)
    }

    pub fn with_default_fn<Svc, Ca, Impl, E, F>(self, f: F) -> Self
    where 
        Svc: Reflect + Send + Sync + ?Sized, 
        Ca: Reflect, 
        Impl: Resolve + Unsize<Svc>,
        E: StdError + Send + Sync + 'static,
        F: Fn(&Self) -> StdResult<Impl, E> + Send + Sync + 'static
    {
        self.with_alternative_fn::<Svc, Main, Ca, Impl, E, F>(f)
    }

    pub fn with_default<Svc, Ca, Impl>(self) -> Self
    where 
        Svc: Reflect + Send + Sync + ?Sized, 
        Ca: Reflect,
        Impl: Resolve + Unsize<Svc>,
        Self: ResolveStart<Impl>,
    {
        self.with_alternative::<Svc, Main, Ca, Impl>()
    }

    pub fn with_cache<Ca>(self) -> Self 
        where Ca: Reflect
    {
        let name = cache_name_to_opt(Ca::name()).unwrap(); // TODO errmsg
        self.map(|this| WithCache::new(this, name))
    }

    pub fn with_redirects(self, rules: RedirectRules) -> Self {
        self.map(|this| WithRedirects::new(this, rules))
    }
}

// ++++++++++++++++++++ TypedInstanceHandle ++++++++++++++++++++

pub struct TypedInstanceHandle<Svc: ?Sized, Alt = Main> {
    obj: InstanceHandle,
    _p: PhantomData<fn(Svc, Alt)>,
}

impl<Svc, Alt> TypedInstanceHandle<Svc, Alt>
    where Svc: Reflect + Instance + ?Sized, Alt: Reflect
{
    pub fn new(obj: InstanceHandle) -> Result<Self> {
        if let Err(e) = Downcast::<Box<Svc>>::downcast_ref(&*obj) {
            return Err(Error::TypeMismatch(Svc::name().to_owned(), Alt::name().to_owned(), e))
        }
        Ok(Self{ obj, _p: PhantomData })
    }
}

impl<Svc, Alt> Clone for TypedInstanceHandle<Svc, Alt>
    where Svc: Instance + ?Sized
{
    fn clone(&self) -> Self {
        Self{ obj: self.obj.clone(), _p: PhantomData }
    }
}

impl<Svc, Alt> Resolve for TypedInstanceHandle<Svc, Alt>
    where Svc: Reflect + Instance + ?Sized, Alt: Reflect
{
    type Depend = Composition;
    type Error = Error;
    fn resolve(ref comp: Self::Depend) -> Result<Self> {
        comp.instantiate(Svc::name().to_owned(), Alt::name().to_owned())
            .and_then(Self::new)
    }
}

impl<Svc, Alt> Deref for TypedInstanceHandle<Svc, Alt> 
    where Svc: Instance + ?Sized
{
    type Target = Svc;
    fn deref(&self) -> &Self::Target { 
        Downcast::<Box<Svc>>::downcast_ref(&*self.obj).unwrap()
    }
}

