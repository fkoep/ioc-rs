extern crate downcast;
extern crate once_cell;
extern crate variadic_generics;

use async_trait::async_trait;
use downcast::{AnySync, TypeMismatch};
use once_cell::sync::OnceCell;
use variadic_generics::va_expand;
use std::collections::HashMap;
use std::error::{self, Error as StdError};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/* Current TODOs:
 *
 * 1. write tests
 * 2. figure out which parts of the library should be hidden
 * 3. detect and prevent hangups caused by mutual dependencies
 * 4. do we want TransientInstancers besides SingletonInstancers?
 */

// errors --------------------------------------------------

pub extern crate anyhow;
pub use anyhow::{Result, Error};

#[derive(Debug)]
pub struct InstancerNotFoundError {
    pub service_name: String
}

impl InstancerNotFoundError {
    pub fn new(service_name: String) -> Self {
        Self{ service_name }
    }
}

impl fmt::Display for InstancerNotFoundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "instancer for {} not found", self.service_name)
    }
}

impl error::Error for InstancerNotFoundError {}

#[derive(Debug)]
pub struct InstanceCreationError {
    pub service_name: String,
    pub creation_error: Error,
}

impl InstanceCreationError {
    pub fn new(service_name: String, creation_error: Error) -> Self {
        Self{ service_name, creation_error }
    }
}

impl fmt::Display for InstanceCreationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "instance creation for {} failed: {}", self.service_name, self.creation_error)
    }
}

impl error::Error for InstanceCreationError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
       Some(&*self.creation_error)
    }
}

#[derive(Debug)]
pub struct InstanceTypeError {
    pub service_name: String,
    pub type_mismatch: TypeMismatch,
}

impl InstanceTypeError {
    pub fn new(service_name: String, type_mismatch: TypeMismatch) -> Self {
        Self{ service_name, type_mismatch }
    }
}

impl fmt::Display for InstanceTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "wrong type for instance of {}: {}", self.service_name, self.type_mismatch)
    }
}

impl error::Error for InstanceTypeError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
       Some(&self.type_mismatch)
    }
}


// Resolve, ResolveStart --------------------------------------------------

#[async_trait]
pub trait Resolve: Send + Sized + 'static {
    type Deps: Send;
    fn resolve(deps: Self::Deps) -> Result<Self>;
    async fn resolve_async(deps: Self::Deps) -> Result<Self>;
}

/// Careful when using this trait, or you'll be in for a world of stack
/// overflows.
#[async_trait]
pub trait ResolveStart<R>: Sync {
    fn resolve_start(&self) -> Result<R>;
    async fn resolve_start_async(&self) -> Result<R>;
}

#[async_trait]
impl<X: Sync> ResolveStart<()> for X {
    fn resolve_start(&self) -> Result<()> { Ok(()) }
    async fn resolve_start_async(&self) -> Result<()> { Ok(()) }
}

#[async_trait]
impl<R, X> ResolveStart<R> for X
    where R: Resolve, X: ResolveStart<R::Deps>
{
    fn resolve_start(&self) -> Result<R> {
        R::resolve(<X as ResolveStart<R::Deps>>::resolve_start(self)?)
    }
    async fn resolve_start_async(&self) -> Result<R> {
        R::resolve_async(<X as ResolveStart<R::Deps>>::resolve_start_async(self).await?).await
    }
}

// tuples
va_expand!{ ($va_len:tt) ($($va_idents:ident),+) ($($va_indices:tt),+)
    #[async_trait]
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
        async fn resolve_start_async(&self) -> Result<($($va_idents,)+)> { 
            Ok(($(
                $va_idents::resolve_async(<X as ResolveStart<$va_idents::Deps>>::resolve_start_async(self).await?).await?,
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

#[async_trait]
pub trait Middleware: Send + Sync + 'static {
    fn instantiate(&self, req: InstantiationRequest) -> Result<InstanceRef>;
    async fn instantiate_async(&self, req: InstantiationRequest) -> Result<InstanceRef>;
}

#[async_trait]
impl ResolveStart<Arc<dyn Middleware>> for Arc<dyn Middleware> {
    fn resolve_start(&self) -> Result<Arc<dyn Middleware>> { Ok(self.clone()) }
    async fn resolve_start_async(&self) -> Result<Arc<dyn Middleware>> { Ok(self.clone()) }
}

// InstanceRef & TypedInstanceRef --------------------------------------------------

pub type InstanceRef = Arc<dyn AnySync>;

#[allow(type_alias_bounds)]
pub type TypedInstanceRef<S: ?Sized> = Arc<Box<S>>;

#[async_trait]
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

    async fn resolve_async(top: Self::Deps) -> Result<Self> {
        let req = InstantiationRequest{
            top: top.clone(),
            service_name: S::service_name(),
            shadow_levels: Some((S::service_name(), 1)).into_iter().collect(),
        };
        top.instantiate_async(req).await?
            .downcast_arc::<Box<S>>()
            .map_err(|err| InstanceTypeError::new(S::service_name(), err.type_mismatch()).into())
    }
}

// Middleware: ContainerRoot --------------------------------------------------

struct ContainerRoot;

#[async_trait]
impl Middleware for ContainerRoot {
    fn instantiate(&self, req: InstantiationRequest) -> Result<InstanceRef> {
        Err(InstancerNotFoundError::new(req.service_name).into())
    }
    async fn instantiate_async(&self, req: InstantiationRequest) -> Result<InstanceRef> {
        self.instantiate(req)
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

#[async_trait]
impl Middleware for InstancerShadow {
    fn instantiate(&self, mut req: InstantiationRequest) -> Result<InstanceRef> {
        if self.shadowed_service_name == req.service_name {
            req.increment_shadow(&self.shadowed_service_name)
        }
        self.prev.instantiate(req)
    }
    async fn instantiate_async(&self, mut req: InstantiationRequest) -> Result<InstanceRef> {
        if self.shadowed_service_name == req.service_name {
            req.increment_shadow(&self.shadowed_service_name)
        }
        self.prev.instantiate_async(req).await
    }
}

// Middleware: (Async)SingletonInstancer  --------------------------------------------------

#[allow(type_alias_bounds)]
type CreationFn<T: ?Sized> = Arc<dyn (Fn(&Arc<dyn Middleware>) -> Result<Box<T>>) + Send + Sync>;
#[allow(type_alias_bounds)]
type AsyncCreationFn<T: ?Sized> = Arc<dyn (Fn(&'_ Arc<dyn Middleware>) -> Pin<Box<dyn Future<Output = Result<Box<T>>> + Send + '_>>) + Send + Sync>;

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

#[async_trait]
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
    
    async fn instantiate_async(&self, req: InstantiationRequest) -> Result<InstanceRef> {
        self.instantiate(req)
    }
}

struct AsyncSingletonInstancer<T: ?Sized> {
    prev: Arc<dyn Middleware>,
    creation_fn: AsyncCreationFn<T>,
    #[allow(clippy::redundant_allocation)]
    instance: futures::lock::Mutex<Option<Arc<Box<T>>>>,
    service_name: String,
}

impl<T> AsyncSingletonInstancer<T>
    where T: Service + ?Sized
{
    fn new(prev: Arc<dyn Middleware>, creation_fn: AsyncCreationFn<T>) -> Self {
        let service_name = T::service_name();
        Self{ prev, creation_fn, instance: futures::lock::Mutex::new(None), service_name } 
    }
}

#[async_trait]
impl<T> Middleware for AsyncSingletonInstancer<T>
    where T: Service + ?Sized
{
    fn instantiate(&self, req: InstantiationRequest) -> Result<InstanceRef> {
        Err(InstanceCreationError::new(
                req.service_name, 
                anyhow::format_err!("cannot call async creation function from non-async code, use async functions instead")
        ).into())
    }
    
    async fn instantiate_async(&self, mut req: InstantiationRequest) -> Result<InstanceRef> {
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
        let mut guard = self.instance.lock().await;
        if guard.is_none() {
            let inst = (self.creation_fn)(&shadowed_top).await
                .map(Arc::new)
                .map_err(|err| InstanceCreationError::new(self.service_name.clone(), err))?;
            *guard = Some(inst);
        }
        Ok(guard.as_ref().cloned().unwrap())
    }
}

// Container --------------------------------------------------

#[derive(Clone)]
pub struct Container {
    top: Arc<dyn Middleware>,
}

#[async_trait]
impl Resolve for Container {
    type Deps = Arc<dyn Middleware>;
    fn resolve(top: Self::Deps) -> Result<Self> {
        Ok(Container{ top })
    }
    async fn resolve_async(top: Self::Deps) -> Result<Self> {
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

    pub fn with_singleton_async<S, Args, Fut, F>(&self, creation_fn: F) -> Self
    where 
        S: Service + ?Sized,
        Arc<dyn Middleware>: ResolveStart<Args>,
        Args: Send,
        Fut: Future<Output = Result<Box<S>>> + Send,
        F: Fn(Args) -> Fut + Send + Sync + Copy + 'static
    {
    	let creation_fn: AsyncCreationFn<S> = Arc::new(move |mw| {
            Box::pin(async move { creation_fn(mw.resolve_start_async().await?).await })
        });
        Self::new(Arc::new(AsyncSingletonInstancer::<S>::new(self.top.clone(), creation_fn)))
    }

    pub fn with_singleton_async_ok<S, Args, Fut, F>(&self, creation_fn: F) -> Self
    where 
        S: Service + ?Sized,
        Arc<dyn Middleware>: ResolveStart<Args>,
        Args: Send,
        Fut: Future<Output = Box<S>> + Send,
        F: Fn(Args) -> Fut + Send + Sync + Copy + 'static
    {
    	let creation_fn: AsyncCreationFn<S> = Arc::new(move |mw| {
            Box::pin(async move { Ok(creation_fn(mw.resolve_start_async().await?).await) })
        });
        Self::new(Arc::new(AsyncSingletonInstancer::<S>::new(self.top.clone(), creation_fn)))
    }

    pub fn resolve<X>(&self) -> Result<X>
        where Arc<dyn Middleware>: ResolveStart<X>
    {
        self.top.resolve_start()
    }

    pub async fn resolve_async<X>(&self) -> Result<X>
        where Arc<dyn Middleware>: ResolveStart<X>
    {
        self.top.resolve_start_async().await
    }
}

// helpful aliases --------------------------------------------------

pub use TypedInstanceRef as I;
pub fn container() -> Container { Default::default() } 

