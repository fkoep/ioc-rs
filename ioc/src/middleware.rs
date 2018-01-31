use downcast::{Any, TypeMismatch};
use std::sync::{Arc, RwLock};
use std::collections::{HashSet, HashMap};
use std::error::Error as StdError;
use std::result::Result as StdResult;

pub type GenericError = Box<StdError + Send + Sync>;
pub type GenericResult<T> = StdResult<T, GenericError>;

quick_error!{
    // TODO add shadow-level
    #[derive(Debug)]
    pub enum Error {
        InstanceNotFound(svc: String, alt: String){
            description("Instance Not Found")
            display(this) -> ("[{}][{}] {}", svc, alt, this.description())
        }
        CreationError(svc: String, alt: String, err: GenericError){
            description("Instance Creation Error")
            display(this) -> ("[{}][{}] {}: {}", svc, alt, this.description(), err)
            cause(&**err)
        }
        TypeMismatch(svc: String, alt: String, err: TypeMismatch){
            description("Instance Type Mismatch")
            display(this) -> ("[{}[{}] {}: {}", svc, alt, this.description(), err)
            cause(err)
        }
        Other(err: GenericError){
            from()
            description(err.description())
            display("{}", err)
            cause(&**err)
        }
    }
}

pub type Result<T> = StdResult<T, Error>;

// ++++++++++++++++++++ Middleware ++++++++++++++++++++

pub trait Instance: Any + Send + Sync {}

impl<T> Instance for T
    where T: Any + Send + Sync + ?Sized
{}

downcast!(Instance);

pub type InstanceObject = Box<Instance>;
pub type InstanceHandle = Arc<Instance>;

pub struct Request {
    pub top: Arc<Middleware>,
    pub service: String,
    pub alternative: String,
    pub shadow: u32, 
    /// TODO naming? `interested_caches`?
    pub outer_caches: HashSet<String>,
}

impl Request {
     pub fn new(top: Arc<Middleware>, service: String, alternative: String) -> Self {
        Self{ top, service, alternative, shadow: 0, outer_caches: Default::default() }
     }
 }

#[derive(Constructor)]
pub struct Response {
    pub handle: InstanceHandle,
    pub for_cache: Option<String>,
    //TODO, also check in WithCache
    //pub shadow: u32
}

pub trait Middleware: Send + Sync + 'static {
    fn instantiate(&self, req: Request) -> Result<Response>;
}

// ++++++++++++++++++++ Root ++++++++++++++++++++

pub struct Root;

impl Middleware for Root {
    fn instantiate(&self, req: Request) -> Result<Response> {
        Err(Error::InstanceNotFound(req.service, req.alternative))
    }
}

// ++++++++++++++++++++ WithFactory ++++++++++++++++++++

// TODO Naming? 
#[derive(Constructor)]
struct InCreateFn {
    inner: Arc<Middleware>,
    service: String,
    alternative: String,
}

impl Middleware for InCreateFn {
    fn instantiate(&self, mut req: Request) -> Result<Response> {
        if req.service == self.service && req.alternative == self.alternative {
            req.shadow += 1;
        }
        self.inner.instantiate(req)
    }
}

pub type CreateFn = Fn(Arc<Middleware>) -> GenericResult<InstanceObject> + Send + Sync;

#[derive(Constructor)]
pub struct WithFactory {
    inner: Arc<Middleware>,
    service: String,
    alternative: String,
    create_fn: Box<CreateFn>,
    for_cache: Option<String>,
}

impl Middleware for WithFactory {
    fn instantiate(&self, mut req: Request) -> Result<Response> {
        if req.service == self.service && req.alternative == self.alternative {
            if req.shadow == 0 {
                let new_top = Arc::new(InCreateFn::new(self.inner.clone(), self.service.clone(), self.alternative.clone()));
                match (self.create_fn)(new_top) {
                    Ok(obj) => Ok(Response::new(obj.into(), self.for_cache.clone())),
                    Err(e) => Err(Error::CreationError(req.service, req.alternative, e))
                }
            } else {
                req.shadow -= 1;
                self.inner.instantiate(req)
            }
        } else {
            self.inner.instantiate(req)
        }
    }
}

// ++++++++++++++++++++ WithCache ++++++++++++++++++++

#[derive(Default)]
struct Cache {
    repos: RwLock<HashMap<String, HashMap<String, InstanceHandle>>>
}

impl Cache {
    fn insert_new(&self, svc: String, alt: String, obj: InstanceHandle){
        let mut repos = self.repos.write().unwrap();
        repos.entry(svc)
            .or_insert_with(Default::default)
            .entry(alt)
            .or_insert(obj);
    }
    fn get(&self, svc: &str, alt: &str) -> Option<InstanceHandle> {
        let repos = self.repos.read().unwrap();
        repos.get(svc).and_then(|repo| repo.get(alt).map(|i| i.clone()))
    }
    // fn foreach<R, F>(&self, mut f: F) -> StdResult<(), R>
    //     where F: FnMut(&String, &String, &InstanceHandle) -> StdResult<(), R>
    // {
    //     let repos = self.repos.read().unwrap();
    //     for (svc, repo) in &*repos {
    //         for (alt, handle) in repo {
    //             f(svc, alt, handle)?;
    //         }
    //     }
    //     Ok(())
    // }
}

pub struct WithCache {
    inner: Arc<Middleware>,
    name: String,
    cache: Cache,
}

impl WithCache {
    pub fn new(inner: Arc<Middleware>, name: String) -> Self { 
        Self{ inner, name, cache: Default::default() } 
    }
}

impl Middleware for WithCache {
    fn instantiate(&self, mut req: Request) -> Result<Response> {
        if req.shadow != 0 {
            return self.inner.instantiate(req)
        }

        // if contained in cache, simply retrieve
        if let Some(obj) = self.cache.get(&req.service, &req.alternative) {
            return Ok(Response::new(obj, None));
        }

        // if a cache with our name is already interested, it's none of our business anymore
        if req.outer_caches.contains(&self.name) {
            return self.inner.instantiate(req);
        }

        // we may be interested in this response
        req.outer_caches.insert(self.name.clone());

        let service = req.service.clone(); // TODO can we get rid of this?
        let alternative = req.alternative.clone(); // TODO can we get rid of this?

        let mut resp = self.inner.instantiate(req)?;
        if resp.for_cache.as_ref() == Some(&self.name) {
            self.cache.insert_new(service.clone(), alternative.clone(), resp.handle.clone());
            resp.for_cache = None;
        }
        return Ok(resp)
    }
}

// ++++++++++++++++++++ WithRedirects ++++++++++++++++++++

#[derive(Default)]
pub struct RedirectRules {
    pub service: HashMap<String, String>,
    pub alternative: HashMap<String, String>,
    pub for_cache: HashMap<Option<String>, Option<String>>,
}

#[derive(Constructor)]
pub struct WithRedirects {
    inner: Arc<Middleware>,
    rules: RedirectRules,
}

impl Middleware for WithRedirects {
    fn instantiate(&self, mut req: Request) -> Result<Response> {
        if req.shadow != 0 {
            return self.inner.instantiate(req)
        }
        if let Some(dest) = self.rules.service.get(&req.service) {
            req.service = dest.clone();
        }
        if let Some(dest) = self.rules.alternative.get(&req.alternative) {
            req.alternative = dest.clone();
        }
        let mut resp = self.inner.instantiate(req)?;
        if let Some(dest) = self.rules.for_cache.get(&resp.for_cache) {
            resp.for_cache = dest.clone();
        }
        Ok(resp)
    }
}
