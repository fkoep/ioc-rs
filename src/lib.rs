use downcast::AnySync;
use once_cell::sync::OnceCell;
use variadic_generics::va_expand;
use std::collections::HashMap;
use std::sync::Arc;

pub extern crate anyhow;

mod common;
pub use common::*;

#[cfg(feature = "async")]
pub mod r#async;

/* Current TODOs:
 *
 * 1. write tests
 * 2. figure out which parts of the library should be hidden
 * 3. detect and prevent hangups caused by mutual dependencies
 * 4. do we want TransientInstancers besides SingletonInstancers?
 */

// Resolve, ResolveStart --------------------------------------------------

pub trait Resolve: Send + Sized + 'static {
    type Deps: Send;
    fn resolve(deps: Self::Deps) -> Result<Self>;
}

/// Careful when using this trait, or you'll be in for a world of stack
/// overflows.
pub trait ResolveStart<R>: Sync {
    fn resolve_start(&self) -> Result<R>;
}

impl<X: Sync> ResolveStart<()> for X {
    fn resolve_start(&self) -> Result<()> { Ok(()) }
}

impl<R, X> ResolveStart<R> for X
    where R: Resolve, X: ResolveStart<R::Deps>
{
    fn resolve_start(&self) -> Result<R> {
        R::resolve(<X as ResolveStart<R::Deps>>::resolve_start(self)?)
    }
}

// tuples
va_expand!{ ($va_len:tt) ($($va_idents:ident),+) ($($va_indices:tt),+)
    impl<$($va_idents,)+ X> ResolveStart<($($va_idents,)+)> for X
    where 
        $($va_idents: Resolve,)+
        $(X: ResolveStart<$va_idents::Deps>,)+
    {
        fn resolve_start(&self) -> Result<($($va_idents,)+)> { 
            Ok(($(
                $va_idents::resolve(<X as ResolveStart<$va_idents::Deps>>::resolve_start(self)?)?,
            )+))
        }
    }
}

// Middleware --------------------------------------------------

#[derive(Clone)]
pub struct InstantiationRequest {
    pub top: Arc<dyn Middleware>,
    pub service_name: String,
    pub shadow_levels: HashMap<String, usize>,
}

impl InstantiationRequest {
    fn increment_shadow(&mut self, service_name: &str){
        let level = self.shadow_levels.entry(service_name.to_owned())
            .or_insert(0);
        *level += 1;
    }
    
    /// returns true if successfully decremented
    fn decrement_shadow(&mut self, service_name: &str) -> bool {
        self.shadow_levels.get_mut(service_name)
            .map(|level| level.saturating_sub(1))
            .unwrap_or(1) != 0
    }
}

pub trait Middleware: Send + Sync + 'static {
    fn instantiate(&self, req: InstantiationRequest) -> Result<InstanceRef>;
}

impl ResolveStart<Arc<dyn Middleware>> for Arc<dyn Middleware> {
    fn resolve_start(&self) -> Result<Arc<dyn Middleware>> { Ok(self.clone()) }
}

impl<S> Resolve for TypedInstanceRef<S>
    where S: Service + ?Sized
{
    type Deps = Arc<dyn Middleware>;

    fn resolve(top: Self::Deps) -> Result<Self> {
        let req = InstantiationRequest{
            top: top.clone(),
            service_name: S::service_name(),
            shadow_levels: Some((S::service_name(), 1)).into_iter().collect(),
        };
        top.instantiate(req)?
            .downcast_arc::<Box<S>>()
            .map_err(|err| InstanceTypeError::new(S::service_name(), err.type_mismatch()).into())
    }
}

// Middleware: ContainerRoot --------------------------------------------------

struct ContainerRoot;

impl Middleware for ContainerRoot {
    fn instantiate(&self, req: InstantiationRequest) -> Result<InstanceRef> {
        Err(InstancerNotFoundError::new(req.service_name).into())
    }
}

// Middleware: InstancerShadow --------------------------------------------------

struct InstancerShadow {
    prev: Arc<dyn Middleware>,
    shadowed_service_name: String
}

impl InstancerShadow {
    fn new(prev: Arc<dyn Middleware>, shadowed_service_name: String) -> Self {
        Self{ prev, shadowed_service_name }
    }
}

impl Middleware for InstancerShadow {
    fn instantiate(&self, mut req: InstantiationRequest) -> Result<InstanceRef> {
        if self.shadowed_service_name == req.service_name {
            req.increment_shadow(&self.shadowed_service_name)
        }
        self.prev.instantiate(req)
    }
}

// Middleware: SingletonInstancer  --------------------------------------------------

#[allow(type_alias_bounds)]
type CreationFn<T: ?Sized> = Arc<dyn (Fn(&Arc<dyn Middleware>) -> Result<Box<T>>) + Send + Sync>;

struct SingletonInstancer<T: ?Sized> {
    prev: Arc<dyn Middleware>,
    creation_fn: CreationFn<T>,
    #[allow(clippy::redundant_allocation)]
    instance: OnceCell<Arc<Box<T>>>,
    service_name: String,
}

impl<T> SingletonInstancer<T>
    where T: Service + ?Sized
{
    fn new(prev: Arc<dyn Middleware>, creation_fn: CreationFn<T>) -> Self {
        let service_name = T::service_name();
        Self{ prev, creation_fn, instance: OnceCell::new(), service_name } 
    }
}

impl<T> Middleware for SingletonInstancer<T>
    where T: Service + ?Sized
{
    fn instantiate(&self, mut req: InstantiationRequest) -> Result<InstanceRef> {
        // if different service or shadowed, pass request (with one less shadow level) up the chain
        if req.service_name != self.service_name
        || req.decrement_shadow(&self.service_name)
        {
    	    return self.prev.instantiate(req)
        }
        
        // increase shadow level
        req.increment_shadow(&self.service_name);
        let shadowed_top: Arc<dyn Middleware> = Arc::new(InstancerShadow::new(req.top, self.service_name.clone()));
        
        // recall or create instance
        self.instance.get_or_try_init(move || (self.creation_fn)(&shadowed_top).map(Arc::new))
            .map(|inst| inst.clone() as Arc<dyn AnySync>)
            .map_err(|err| InstanceCreationError::new(self.service_name.clone(), err).into())
    }
}

// Container --------------------------------------------------

#[derive(Clone)]
pub struct Container {
    top: Arc<dyn Middleware>,
}

impl Resolve for Container {
    type Deps = Arc<dyn Middleware>;
    fn resolve(top: Self::Deps) -> Result<Self> {
        Ok(Container{ top })
    }
}

impl Default for Container {
    fn default() -> Self{ Self::new(Arc::new(ContainerRoot)) }
}

impl Container {
    pub fn new(top: Arc<dyn Middleware>) -> Self {
        Self{ top }
    }

    pub fn with_singleton<S, Args, F>(&self, creation_fn: F) -> Self
    where 
        S: Service + ?Sized,
        Arc<dyn Middleware>: ResolveStart<Args>,
        F: Fn(Args) -> Result<Box<S>> + Send + Sync + 'static
    {
    	let creation_fn: CreationFn<S> = Arc::new(move |mw: &Arc<dyn Middleware>| {
            creation_fn(mw.resolve_start()?)
        });
        Self::new(Arc::new(SingletonInstancer::new(self.top.clone(), creation_fn)))
    }

    pub fn with_singleton_ok<S, Args, F>(&self, creation_fn: F) -> Self
    where
        S: Service + ?Sized,
        Arc<dyn Middleware>: ResolveStart<Args>,
        F: Fn(Args) -> Box<S> + Send + Sync + 'static
    {
    	let creation_fn: CreationFn<S> = Arc::new(move |mw: &Arc<dyn Middleware>| {
            Ok(creation_fn(mw.resolve_start()?))
        });
        Self::new(Arc::new(SingletonInstancer::new(self.top.clone(), creation_fn)))
    }

    pub fn resolve<X>(&self) -> Result<X>
        where Arc<dyn Middleware>: ResolveStart<X>
    {
        self.top.resolve_start()
    }
}

pub fn container() -> Container { Default::default() } 
