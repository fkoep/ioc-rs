#[macro_use]
extern crate derive_more;
extern crate downcast;
extern crate once_cell;
extern crate variadic_generics;

use downcast::{AnySync, TypeMismatch};
use once_cell::sync::OnceCell;
use variadic_generics::va_expand;
use std::error::Error as StdError;
use std::result::Result as StdResult;
use std::sync::Arc;
use std::collections::HashMap;

// errors --------------------------------------------------

pub type Error = Box<dyn StdError>;
pub type Result<T> = StdResult<T, Error>;

#[derive(Debug, Display, Error, Constructor)]
#[display(fmt = "instancer for {} not found", service_name)]
pub struct InstancerNotFoundError {
    pub service_name: String
}

#[derive(Debug, Display, Error, Constructor)]
#[display(fmt = "instance creation for {} failed: {}", service_name, creation_error)]
pub struct InstanceCreationError {
    pub service_name: String,
    
    //FIXME https://github.com/JelteF/derive_more/issues/122
    // #[error(source)]
    pub creation_error: Error,
}

#[derive(Debug, Display, Error, Constructor)]
#[display(fmt = "wrong type for instance of {}: {}", service_name, type_mismatch)]
pub struct InstanceTypeError {
    pub service_name: String,
    #[error(source)]
    pub type_mismatch: TypeMismatch,
}

// Resolve & ResolveStart --------------------------------------------------

pub trait Resolve: Sized + 'static {
    type Deps;
    fn resolve(deps: Self::Deps) -> Result<Self>;
}

/// Careful when using this trait, or you'll be in for a world of stack
/// overflows.
pub trait ResolveStart<R> {
    fn resolve_start(&self) -> Result<R>;
}

impl<X> ResolveStart<()> for X {
    fn resolve_start(&self) -> Result<()> { Ok(()) }
}

impl<R, X> ResolveStart<R> for X
    where R: Resolve, X: ResolveStart<R::Deps>
{
    fn resolve_start(&self) -> Result<R> {
        Ok(R::resolve(<X as ResolveStart<R::Deps>>::resolve_start(self)?)?)
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

// Service --------------------------------------------------

pub trait Service: Send + Sync + 'static {
    fn service_name() -> String {
        std::any::type_name::<Self>()
            .replace("dyn ", "")
            .replace("::", ".")
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
    pub fn increment_shadow(&mut self, service_name: &str){
        let level = self.shadow_levels.entry(service_name.to_owned())
            .or_insert(0);
        *level += 1;
    }
    
    /// returns true if successfully decremented
    pub fn decrement_shadow(&mut self, service_name: &str) -> bool {
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

// InstanceRef & TypedInstanceRef --------------------------------------------------

pub type InstanceRef = Arc<dyn AnySync>;

#[allow(type_alias_bounds)]
pub type TypedInstanceRef<S: ?Sized> = Arc<Box<S>>;

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

pub struct ContainerRoot;

impl Middleware for ContainerRoot {
    fn instantiate(&self, req: InstantiationRequest) -> Result<InstanceRef> {
        return Err(InstancerNotFoundError::new(req.service_name).into())
    }
}

// Middleware: SingletonInstance, TransientInstancer  --------------------------------------------------

pub struct InstancerShadow {
    prev: Arc<dyn Middleware>,
    shadowed_service_name: String
}

impl InstancerShadow {
    pub fn new(prev: Arc<dyn Middleware>, shadowed_service_name: String) -> Self {
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

#[allow(type_alias_bounds)]
pub type CreationFn<T: ?Sized> = Arc<dyn (Fn(&Arc<dyn Middleware>) -> Result<Box<T>>) + Send + Sync>;

pub struct SingletonInstancer<T: ?Sized> {
    prev: Arc<dyn Middleware>,
    creation_fn: CreationFn<T>,
    instance: OnceCell<Arc<Box<T>>>,
    service_name: String,
}

impl<T> SingletonInstancer<T>
    where T: Service + ?Sized
{
    pub fn new(prev: Arc<dyn Middleware>, creation_fn: CreationFn<T>) -> Self {
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

pub struct TransientInstancer<T: ?Sized> {
    prev: Arc<dyn Middleware>,
    creation_fn: CreationFn<T>,
    service_name: String,
}

impl<T> TransientInstancer<T>
    where T: Service + ?Sized
{
    pub fn new(prev: Arc<dyn Middleware>, creation_fn: CreationFn<T>) -> Self {
        let service_name = T::service_name();
        Self{ prev, creation_fn, service_name } 
    }
}

impl<T> Middleware for TransientInstancer<T>
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
        
        // create instance
        (self.creation_fn)(&shadowed_top)
            .map(|inst| Arc::new(inst) as InstanceRef)
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

    pub fn with_singleton<S, Args>(&self, f: impl Fn(Args) -> Result<Box<S>> + Send + Sync + 'static) -> Self
        where S: Service + ?Sized, Arc<dyn Middleware>: ResolveStart<Args>
    {
    	let creation_fn = move |mw: &Arc<dyn Middleware>| Ok(f(mw.resolve_start()?)?);
        Self::new(Arc::new(SingletonInstancer::new(self.top.clone(), Arc::new(creation_fn))))
    }

    pub fn with_singleton_ok<S, Args>(&self, f: impl Fn(Args) -> Box<S> + Send + Sync + 'static) -> Self
        where S: Service + ?Sized, Arc<dyn Middleware>: ResolveStart<Args>
    {
    	let creation_fn = move |mw: &Arc<dyn Middleware>| Ok(f(mw.resolve_start()?));
        Self::new(Arc::new(SingletonInstancer::new(self.top.clone(), Arc::new(creation_fn))))
    }

    pub fn with_transient<S, Args>(&self, f: impl Fn(Args) -> Result<Box<S>> + Send + Sync + 'static) -> Self
        where S: Service + ?Sized, Arc<dyn Middleware>: ResolveStart<Args>
    {
    	let creation_fn = move |mw: &Arc<dyn Middleware>| Ok(f(mw.resolve_start()?)?);
        Self::new(Arc::new(TransientInstancer::new(self.top.clone(), Arc::new(creation_fn))))
    }

    pub fn with_transient_ok<S, Args>(&self, f: impl Fn(Args) -> Box<S> + Send + Sync + 'static) -> Self
        where S: Service + ?Sized, Arc<dyn Middleware>: ResolveStart<Args>
    {
    	let creation_fn = move |mw: &Arc<dyn Middleware>| Ok(f(mw.resolve_start()?));
        Self::new(Arc::new(TransientInstancer::new(self.top.clone(), Arc::new(creation_fn))))
    }
    
    pub fn resolve<X>(&self) -> Result<X>
        where Arc<dyn Middleware>: ResolveStart<X>
    {
        self.top.resolve_start()
    }
}

// helpful aliases --------------------------------------------------

pub use TypedInstanceRef as I;
pub fn container() -> Container { Default::default() } 

