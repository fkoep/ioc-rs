use super::builder::Builder;
use super::transient::Transient;
// use super::nested::Nested;
use internal::Node;
use resolve::{self, ResolveRoot};
use std::marker::PhantomData;
use std::sync::Arc;

pub struct Root(Arc<Node>);

impl Root {
    fn new(name: String) -> Self { Root(Arc::new(Node::root(name))) }

    /* TODO reduce code duplication (nested.rs) */

    fn _request<X, F>(&self, name: Option<String>, f: F) -> resolve::Result<X>
        where F: FnOnce(Builder<Transient>) -> resolve::Result<X>
    {
        let transient_node = Arc::new(Node::transient_child(self.0.clone(), name));
        let transient = Transient::rewrap(transient_node);
        f(Builder::new(transient))
    }
    pub fn request<X, F>(&self, f: F) -> resolve::Result<X>
        where F: FnOnce(Builder<Transient>) -> resolve::Result<X>
    {
        self._request(None, f)
    }
    pub fn request_named<S, X, F>(&self, name: S, f: F) -> resolve::Result<X>
        where S: Into<String>, F: FnOnce(Builder<Transient>) -> resolve::Result<X>
    {
        self._request(Some(name.into()), f)
    }

    pub fn resolve<R>(&self) -> resolve::Result<R>
        where Transient: ResolveRoot<R>
    {
        self.request(|t| t.build().resolve::<R>())
    }
    pub fn resolve_named<S, R>(&self, name: S) -> resolve::Result<R>
        where S: Into<String>, Transient: ResolveRoot<R>
    {
        self.request_named(name, |t| t.build().resolve::<R>())
    }

    // pub fn nested<S>(&self, name: S) -> Builder<Nested>
    //     where S: Into<String>
    // {
    //     Builder::new(Nested::new(&self.node, name.into()))
    // }
}

// for Builder
impl AsRef<Arc<Node>> for Root {
    fn as_ref(&self) -> &Arc<Node> { &self.0 }
}

impl Builder<Root> {
    pub fn root<S>(name: S) -> Self
        where S: Into<String>
    {
        Builder::new(Root::new(name.into()))
    }
}
