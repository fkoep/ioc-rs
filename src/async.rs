use super::common::*;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use async_trait::async_trait;
use variadic_generics::va_expand;

// Resolve, ResolveStart --------------------------------------------------

#[async_trait]
pub trait Resolve: Send + Sized + 'static {
    type Deps: Send;
    async fn resolve(deps: Self::Deps) -> Result<Self>;
}

#[async_trait]
pub trait ResolveStart<R>: Sync {
    async fn resolve_start(&self) -> Result<R>;
}

#[async_trait]
impl<X: Sync> ResolveStart<()> for X {
    async fn resolve_start(&self) -> Result<()> { Ok(()) }
}


#[async_trait]
impl<R, X> ResolveStart<R> for X
    where R: Resolve, X: ResolveStart<R::Deps>
{
    async fn resolve_start(&self) -> Result<R> {
        R::resolve(<X as ResolveStart<R::Deps>>::resolve_start(self).await?).await
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
        async fn resolve_start(&self) -> Result<($($va_idents,)+)> { 
            Ok(($(
                $va_idents::resolve(<X as ResolveStart<$va_idents::Deps>>::resolve_start(self).await?).await?,
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

#[async_trait]
pub trait Middleware: Send + Sync + 'static {
    async fn instantiate(&self, req: InstantiationRequest) -> Result<InstanceRef>;
}

#[async_trait]
impl ResolveStart<Arc<dyn Middleware>> for Arc<dyn Middleware> {
    async fn resolve_start(&self) -> Result<Arc<dyn Middleware>> { Ok(self.clone()) }
}

#[async_trait]
impl<S> Resolve for TypedInstanceRef<S>
    where S: Service + ?Sized
{
    type Deps = Arc<dyn Middleware>;

    async fn resolve(top: Self::Deps) -> Result<Self> {
        let req = InstantiationRequest{
            top: top.clone(),
            service_name: S::service_name(),
            shadow_levels: Some((S::service_name(), 1)).into_iter().collect(),
        };
        top.instantiate(req).await?
            .downcast_arc::<Box<S>>()
            .map_err(|err| InstanceTypeError::new(S::service_name(), err.type_mismatch()).into())
    }
}

// Middleware: ContainerRoot --------------------------------------------------

struct ContainerRoot;

#[async_trait]
impl Middleware for ContainerRoot {
    async fn instantiate(&self, req: InstantiationRequest) -> Result<InstanceRef> {
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

#[async_trait]
impl Middleware for InstancerShadow {
    async fn instantiate(&self, mut req: InstantiationRequest) -> Result<InstanceRef> {
        if self.shadowed_service_name == req.service_name {
            req.increment_shadow(&self.shadowed_service_name)
        }
        self.prev.instantiate(req).await
    }
}

// Middleware: SingletonInstancer  --------------------------------------------------

#[allow(type_alias_bounds)]
type CreationFn<T: ?Sized> = Arc<dyn (Fn(&'_ Arc<dyn Middleware>) -> Pin<Box<dyn Future<Output = Result<Box<T>>> + Send + '_>>) + Send + Sync>;

struct SingletonInstancer<T: ?Sized> {
    prev: Arc<dyn Middleware>,
    creation_fn: CreationFn<T>,
    #[allow(clippy::redundant_allocation)]
    instance: futures::lock::Mutex<Option<Arc<Box<T>>>>,
    service_name: String,
}

impl<T> SingletonInstancer<T>
    where T: Service + ?Sized
{
    fn new(prev: Arc<dyn Middleware>, creation_fn: CreationFn<T>) -> Self {
        let service_name = T::service_name();
        Self{ prev, creation_fn, instance: futures::lock::Mutex::new(None), service_name } 
    }
}

#[async_trait]
impl<T> Middleware for SingletonInstancer<T>
    where T: Service + ?Sized
{
    async fn instantiate(&self, mut req: InstantiationRequest) -> Result<InstanceRef> {
        // if different service or shadowed, pass request (with one less shadow level) up the chain
        if req.service_name != self.service_name
        || req.decrement_shadow(&self.service_name)
        {
    	    return self.prev.instantiate(req).await
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
    async fn resolve(top: Self::Deps) -> Result<Self> {
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
        Args: Send,
        F: Fn(Args) -> Result<Box<S>> + Send + Sync + Copy + 'static
    {
    	let creation_fn: CreationFn<S> = Arc::new(move |mw| {
            Box::pin(async move { creation_fn(mw.resolve_start().await?) })
        });
        Self::new(Arc::new(SingletonInstancer::new(self.top.clone(), creation_fn)))
    }

    pub fn with_singleton_ok<S, Args, F>(&self, creation_fn: F) -> Self
    where
        S: Service + ?Sized,
        Arc<dyn Middleware>: ResolveStart<Args>,
        F: Fn(Args) -> Box<S> + Send + Sync + Copy + 'static
    {
    	let creation_fn: CreationFn<S> = Arc::new(move |mw| {
            Box::pin(async move { Ok(creation_fn(mw.resolve_start().await?)) })
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
    	let creation_fn: CreationFn<S> = Arc::new(move |mw| {
            Box::pin(async move { creation_fn(mw.resolve_start().await?).await })
        });
        Self::new(Arc::new(SingletonInstancer::<S>::new(self.top.clone(), creation_fn)))
    }

    pub fn with_singleton_async_ok<S, Args, Fut, F>(&self, creation_fn: F) -> Self
    where 
        S: Service + ?Sized,
        Arc<dyn Middleware>: ResolveStart<Args>,
        Args: Send,
        Fut: Future<Output = Box<S>> + Send,
        F: Fn(Args) -> Fut + Send + Sync + Copy + 'static
    {
    	let creation_fn: CreationFn<S> = Arc::new(move |mw| {
            Box::pin(async move { Ok(creation_fn(mw.resolve_start().await?).await) })
        });
        Self::new(Arc::new(SingletonInstancer::<S>::new(self.top.clone(), creation_fn)))
    }

    pub async fn resolve<X>(&self) -> Result<X>
        where Arc<dyn Middleware>: ResolveStart<X>
    {
        self.top.resolve_start().await
    }
}

pub fn container() -> Container { Default::default() } 
