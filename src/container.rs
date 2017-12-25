use downcast::{Any, TypeMismatch};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error as StdError;
use std::result::Result as StdResult;
use std::sync::{Arc, RwLock};

pub type InstanceObject = Any + Send + Sync;

#[derive(Debug)]
pub enum Error {
    InstanceNotFound(String, String),
    InstantiationError(String, String, Box<StdError>),
    TypeMismatch(String, String, TypeMismatch),
}

pub type Result<T> = StdResult<T, Error>;

// ++++++++++++++++++++ Container ++++++++++++++++++++

pub struct Request {
    pub top: Arc<Container>,
    pub service: String,
    pub variant: String,
    pub interested_caches: BTreeSet<String>,
}

impl Request {
    pub fn new(top: Arc<Container>, service: String, variant: String) -> Self {
        Self{ top, service, variant, interested_caches: Default::default() }
    }
}

pub struct Response {
    pub object: Arc<InstanceObject>,
    pub service: String, //FIXME remove?
    pub variant: String,
    pub for_cache: Option<String>,
}

impl Response {
    pub fn new(object: Arc<InstanceObject>, service: String, variant: String, for_cache: Option<String>) -> Self {
        Self{ object, service, variant, for_cache }
    }
}

pub trait Container: Send + Sync + 'static {
    fn handle(&self, req: Request) -> Result<Response>;
}

// ++++++++++++++++++++ Root ++++++++++++++++++++

pub struct Root(());

impl Root {
    pub fn new() -> Self { Root(()) }
}

impl Container for Root {
    fn handle(&self, mut req: Request) -> Result<Response> {
        Err(Error::InstanceNotFound(req.service, req.variant))
    }
}

// ++++++++++++++++++++ WithCache ++++++++++++++++++++

#[derive(Default)]
struct Cache {
    objects: RwLock<BTreeMap<String, BTreeMap<String, Arc<InstanceObject>>>>
}

impl Cache {
    fn insert_new(&self, svc: String, var: String, obj: Arc<InstanceObject>){
        let mut objects = self.objects.write().unwrap();
        objects.entry(svc)
            .or_insert_with(Default::default)
            .entry(var)
            .or_insert(obj);
    }
    fn get(&self, svc: &str, var: &str) -> Option<Arc<InstanceObject>> {
        let objects = self.objects.read().unwrap();
        objects.get(svc).and_then(|repo| repo.get(var).map(|i| i.clone()))
    }
}

pub struct WithCache {
    inner: Arc<Container>,
    name: String,
    cache: Cache,
}

impl WithCache {
    pub fn new(inner: Arc<Container>, name: String) -> Self {
        Self{ inner, name, cache: Default::default() }
    }
}

impl Container for WithCache {
    fn handle(&self, mut req: Request) -> Result<Response> {
        // if contained in cache, simply retrieve
        if let Some(inst) = self.cache.get(&req.service, &req.variant) {
            return Ok(Response::new(inst, req.service, req.variant, None));
        }

        // if a cache with our name is already interested, it's none of our business anymore
        if req.interested_caches.contains(&self.name) {
            return self.inner.handle(req);
        }

        // we may be interested in this response
        req.interested_caches.insert(self.name.clone());

        let resp = self.inner.handle(req)?;
        if resp.for_cache.as_ref() == Some(&self.name) {
            self.cache.insert_new(resp.service.clone(), resp.variant.clone(), resp.object.clone());
        }
        return Ok(resp)
    }
}

