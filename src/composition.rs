use reflect::Reflect;
use resolve::*;
use middleware::*;
use downcast::Downcast;
use std::collections::BTreeMap;
use std::marker::{PhantomData, Unsize};
use std::sync::Arc;
use std::ops::{Deref};
use std::result::Result as StdResult;
use std::error::Error as StdError;

pub struct Main(());

impl Reflect for Main {
    fn name_init() -> String { "Main".to_owned() }
}

pub struct Return(());

impl Reflect for Return {
    fn name_init() -> String { "Return".to_owned() }
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
//     type Dep;
//     type Error: Error + Send + Sync;
//     fn create(&self, depend: Self::Dep) -> Result<T, Self::Error>;
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
    type Err = Error;
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
    
    pub fn instantiate(&self, svc: &str, alt: &str) -> Result<InstanceHandle> {
        let req = Request::new(self.0.clone(), svc.to_owned(), alt.to_owned());
        let resp = self.0.instantiate(req)?;
        if let Some(for_cache) = resp.for_cache {
            /* TODO return error */
            assert_eq!(for_cache, Return::name(), "Missing cache '{}'", for_cache);
        }
        Ok(resp.handle)
    }

    pub fn instantiate_all(&self, svc: &str) -> Result<InstanceRepo> {
        let mut ret = InstanceRepo::new();
        for alt in self.0.list_alternatives(svc) {
            let inst = self.instantiate(svc, &alt)?;
            ret.insert(alt, inst);
        }
        Ok(ret)
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

    // pub fn with_alternative_simple_obj<Svc, Alt>(self, obj: Svc) -> Self 
    // where 
    //     Svc: Reflect + Send + Sync,
    //     Alt: Reflect
    // {
    //     let svc = Svc::name().to_owned();
    //     let alt = Alt::name().to_owned();
    //     let h: InstanceHandle = Arc::from(box {box obj} as InstanceObject);
    //     self.map(|this| WithFactory::new(this, svc, alt, box move |_| Ok(h.clone()), None))
    // }

    pub fn with_alternative_obj<Svc, Alt, Impl>(self, obj: Impl) -> Self 
    where 
        Svc: Reflect + Send + Sync + ?Sized, 
        Alt: Reflect,
        Impl: Unsize<Svc>
    {
        let svc = Svc::name().to_owned();
        let alt = Alt::name().to_owned();
        let h: InstanceHandle = Arc::from(box {box obj as Box<Svc>} as InstanceObject);
        self.map(|this| WithFactory::new(this, svc, alt, box move |_| Ok(h.clone()), None))
    }

    pub fn with_alternative_fn<Svc, Alt, Ca, Impl, E, F>(self, f: F) -> Self
    where 
        Svc: Reflect + Send + Sync + ?Sized, 
        Alt: Reflect,
        Ca: Reflect, 
        Impl: Unsize<Svc>,
        E: StdError + Send + Sync + 'static,
        F: Fn(&Self) -> StdResult<Impl, E> + Send + Sync + 'static
    {
        let svc = Svc::name().to_owned();
        let alt = Alt::name().to_owned();
        let create_fn = box move |mw| {
            f(&Composition::new(mw))
                .map(|obj| Arc::from(box {box obj as Box<Svc>} as InstanceObject))
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

    pub fn with_main_obj<Svc, Impl>(self, obj: Impl) -> Self 
    where 
        Svc: Reflect + Send + Sync + ?Sized, 
        Impl: Unsize<Svc>
    {
        self.with_alternative_obj::<Svc, Main, Impl>(obj)
    }

    pub fn with_main_fn<Svc, Ca, Impl, E, F>(self, f: F) -> Self
    where 
        Svc: Reflect + Send + Sync + ?Sized, 
        Ca: Reflect, 
        Impl: Unsize<Svc>,
        E: StdError + Send + Sync + 'static,
        F: Fn(&Self) -> StdResult<Impl, E> + Send + Sync + 'static
    {
        self.with_alternative_fn::<Svc, Main, Ca, Impl, E, F>(f)
    }

    pub fn with_main<Svc, Ca, Impl>(self) -> Self
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
    // TODO Naming: inner
    obj: InstanceHandle,
    _p: PhantomData<fn(Svc, Alt)>,
}

impl<Svc, Alt> TypedInstanceHandle<Svc, Alt>
    where Svc: Instance + ?Sized
{
    fn new(obj: InstanceHandle, svc: &str, alt: &str) -> Result<Self> {
        if let Err(e) = Downcast::<Box<Svc>>::downcast_ref(&*obj) {
            return Err(Error::TypeMismatch(svc.to_owned(), alt.to_owned(), e))
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
    type Dep = Composition;
    type Err = Error;
    fn resolve(comp: Self::Dep) -> Result<Self> {
        let svc = Svc::name();
        let alt = Alt::name();
        comp.instantiate(svc, alt)
            .and_then(|h| Self::new(h, svc, alt))
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

// ++++++++++++++++++++ TypedInstanceRepo ++++++++++++++++++++

pub struct TypedInstanceRepo<Svc: ?Sized> {
    inner: BTreeMap<String, TypedInstanceHandle<Svc>>
}

impl<Svc> TypedInstanceRepo<Svc>
    where Svc: Instance + ?Sized
{
    pub fn new(repo: InstanceRepo, svc: &str) -> Result<Self> {
        let mut inner = BTreeMap::new();
        for (alt, handle) in repo {
            inner.insert(alt.clone(), TypedInstanceHandle::new(handle, svc, &alt)?);
        }
        Ok(Self{ inner })
    }
    // TODO remove?
    pub fn into_inner(self) -> BTreeMap<String, TypedInstanceHandle<Svc>> { self.inner }
}

impl<Svc> Clone for TypedInstanceRepo<Svc>
    where Svc: Instance + ?Sized
{
    fn clone(&self) -> Self {
        Self{ inner: self.inner.clone() }
    }
}

impl<Svc> Resolve for TypedInstanceRepo<Svc>
    where Svc: Reflect + Instance + ?Sized
{
    type Dep = Composition;
    type Err = Error;
    fn resolve(comp: Self::Dep) -> Result<Self> {
        let svc = Svc::name();
        Ok(Self::new(comp.instantiate_all(svc)?, svc)?)
    }
}

impl<Svc> Deref for TypedInstanceRepo<Svc> 
    where Svc: Instance + ?Sized
{
    type Target = BTreeMap<String, TypedInstanceHandle<Svc>>;
    fn deref(&self) -> &Self::Target { &self.inner }
}

