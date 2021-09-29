#![feature(unboxed_closures, fn_traits)]

#[macro_use]
extern crate derive_more;
extern crate downcast;
extern crate once_cell;
extern crate variadic_generics;

use downcast::{AnySync, TypeMismatch};
use once_cell::sync::OnceCell;
use variadic_generics::va_expand;
use std::sync::Arc;
use std::error::Error as StdError;
use std::result::Result as StdResult;

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

// Middleware --------------------------------------------------

pub trait Middleware: Send + Sync + 'static {
    fn instantiate(&self, top: &Arc<dyn Middleware>, service: &str) -> Result<InstanceRef>;
}

impl ResolveStart<Arc<dyn Middleware>> for Arc<dyn Middleware> {
    fn resolve_start(&self) -> Result<Arc<dyn Middleware>> { Ok(self.clone()) }
}

// Service --------------------------------------------------

pub trait Service: Send + Sync + 'static {
    fn service_name() -> String {
        std::any::type_name::<Self>()
            .replace("dyn ", "")
            .replace("::", ".")
    }
}

// InstanceRef & TypedInstanceRef --------------------------------------------------

pub type InstanceRef = Arc<dyn AnySync>;

#[allow(type_alias_bounds)]
pub type TypedInstanceRef<S: ?Sized> = Arc<Box<S>>;

impl<S> Resolve for TypedInstanceRef<S>
    where S: Service + ?Sized
{
    type Deps = Arc<dyn Middleware>;
    fn resolve(mw: Self::Deps) -> Result<Self> {
        let service_name = S::service_name();
        mw.instantiate(&mw, &service_name)?
            .downcast_arc::<Box<S>>()
            .map_err(|err| InstanceTypeError::new(service_name.to_owned(), err.type_mismatch()).into())
    }
}

// middleware implementations --------------------------------------------------

#[allow(type_alias_bounds)]
pub type CreationFn<T: ?Sized> = Arc<dyn (Fn(&Arc<dyn Middleware>) -> Result<Box<T>>) + Send + Sync>;

pub struct ContainerRoot;

impl Middleware for ContainerRoot {
    fn instantiate(&self, _top: &Arc<dyn Middleware>, service_name: &str) -> Result<InstanceRef> {
        return Err(InstancerNotFoundError::new(service_name.to_owned()).into())
    }
}

pub struct SingletonInstancer<T: ?Sized> {
    previous: Arc<dyn Middleware>,
    creation_fn: CreationFn<T>,
    instance: OnceCell<Arc<Box<T>>>,
    service_name: String,
}

impl<T> SingletonInstancer<T>
    where T: Service + ?Sized
{
    pub fn new(previous: Arc<dyn Middleware>, creation_fn: CreationFn<T>) -> Self {
        let service_name = T::service_name();
        Self{ previous, creation_fn, instance: OnceCell::new(), service_name } 
    }
}

impl<T> Middleware for SingletonInstancer<T>
    where T: Service + ?Sized
{
    fn instantiate(&self, top: &Arc<dyn Middleware>, service_name: &str) -> Result<InstanceRef> {
        if service_name != self.service_name {
    	    return self.previous.instantiate(top, service_name)
        }
        self.instance.get_or_try_init(|| (self.creation_fn)(top).map(Arc::new))
            .map(|inst| inst.clone() as Arc<dyn AnySync>)
            .map_err(|err| InstanceCreationError::new(service_name.to_owned(), err).into())
    }
}

pub struct TransientInstancer<T: ?Sized> {
    previous: Arc<dyn Middleware>,
    creation_fn: CreationFn<T>,
    service_name: String,
}

impl<T> TransientInstancer<T>
    where T: Service + ?Sized
{
    pub fn new(previous: Arc<dyn Middleware>, creation_fn: CreationFn<T>) -> Self {
        let service_name = T::service_name();
        Self{ previous, creation_fn, service_name } 
    }
}

impl<T> Middleware for TransientInstancer<T>
    where T: Service + ?Sized
{
    fn instantiate(&self, top: &Arc<dyn Middleware>, service_name: &str) -> Result<InstanceRef> {
        if service_name != self.service_name {
    	    return self.previous.instantiate(top, service_name)
        }
        (self.creation_fn)(top)
            .map(|inst| Arc::new(inst) as InstanceRef)
            .map_err(|err| InstanceCreationError::new(service_name.to_owned(), err).into())
    }
}

// Container --------------------------------------------------

#[derive(Clone)]
pub struct Container {
    middleware: Arc<dyn Middleware>,
}

impl Default for Container {
    fn default() -> Self{ Self::new(Arc::new(ContainerRoot)) }
}

impl Container {
    pub fn new(middleware: Arc<dyn Middleware>) -> Self {
        Self{ middleware }
    }
    
    pub fn with<M: Middleware>(&self, f: impl FnOnce(Arc<dyn Middleware>) -> M) -> Self {
        let new_mw = f(self.middleware.clone());
        Self::new(Arc::new(new_mw))
    }

    pub fn with_singleton<S, Args>(&self, f: impl Fn<Args, Output = Result<Box<S>>> + Send + Sync + 'static) -> Self
        where S: Service + ?Sized, Arc<dyn Middleware>: ResolveStart<Args>
    {
    	let creation_fn = move |mw: &Arc<dyn Middleware>| Ok(f.call(mw.resolve_start()?)?);
        self.with(move |mw| SingletonInstancer::new(mw, Arc::new(creation_fn)))
    }

    pub fn with_singleton_ok<S, Args>(&self, f: impl Fn<Args, Output = Box<S>> + Send + Sync + 'static) -> Self
        where S: Service + ?Sized, Arc<dyn Middleware>: ResolveStart<Args>
    {
    	let creation_fn = move |mw: &Arc<dyn Middleware>| Ok(f.call(mw.resolve_start()?));
        self.with(move |mw| SingletonInstancer::new(mw, Arc::new(creation_fn)))
    }

    pub fn with_transient<S, Args>(&self, f: impl Fn<Args, Output = Result<Box<S>>> + Send + Sync + 'static) -> Self
        where S: Service + ?Sized, Arc<dyn Middleware>: ResolveStart<Args>
    {
    	let creation_fn = move |mw: &Arc<dyn Middleware>| Ok(f.call(mw.resolve_start()?)?);
        self.with(move |mw| TransientInstancer::new(mw, Arc::new(creation_fn)))
    }

    pub fn with_transient_ok<S, Args>(&self, f: impl Fn<Args, Output = Box<S>> + Send + Sync + 'static) -> Self
        where S: Service + ?Sized, Arc<dyn Middleware>: ResolveStart<Args>
    {
    	let creation_fn = move |mw: &Arc<dyn Middleware>| Ok(f.call(mw.resolve_start()?));
        self.with(move |mw| TransientInstancer::new(mw, Arc::new(creation_fn)))
    }
    
    pub fn resolve<X>(&self) -> Result<X>
        where Arc<dyn Middleware>: ResolveStart<X>
    {
        self.middleware.resolve_start()
    }
}

// helpful aliases --------------------------------------------------

pub use TypedInstanceRef as I;
pub fn container() -> Container { Default::default() } 

