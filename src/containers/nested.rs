use super::builder::Builder;
use super::transient::Transient;
use internal::Node;
use resolve::{self, ResolveRoot};
use std::marker::PhantomData;
use std::sync::Arc;

pub struct Nested {
    node: Arc<Node>,
}

impl<'a> Nested<'a> {
    pub fn new(parent: &'a Arc<Node>, name: String) -> Self {
        Self{ node: Arc::new(Node::persistent_sub(parent.clone(), name)), _p: PhantomData }
    }

    /* TODO reduce code duplication (root.rs) */

    fn _request<X, F>(&self, name: Option<String>, f: F) -> resolve::Result<X>
        where F: FnOnce(Builder<Transient>) -> resolve::Result<X>
    {
        let transient_node = Arc::new(Node::transient_sub(self.node.clone(), name));
        let transient = Transient::wrap(&transient_node);
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
        where for<'t> Transient<'t>: ResolveRoot<R>
    {
        self.request(|t| t.build().resolve::<R>())
    }
    pub fn resolve_named<S, R>(&self, name: S) -> resolve::Result<R>
        where S: Into<String>, for<'t> Transient<'t>: ResolveRoot<R>
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
impl<'a> AsRef<Arc<Node>> for Nested<'a> {
    fn as_ref(&self) -> &Arc<Node> { &self.node }
}

