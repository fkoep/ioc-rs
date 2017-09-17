use super::builder::Builder;
use super::instance::Instance;
// use super::nested::Nested;
use super::reflect::Reflect;
use internal::Node;
use resolve::{self, ResolveRoot};
use std::sync::Arc;

pub struct Transient(Arc<Node>);

impl Transient {
    pub fn new(parent: Arc<Node>, name: Option<String>) -> Self {
        let node = Arc::new(Node::transient_child(parent, name));
        Transient(node)
    }
    pub fn rewrap(node: Arc<Node>) -> Self {
        assert!(node.is_transient());
        Transient(node)
    }

    pub fn instantiate<T>(
        &self,
        repo_name: &String,
        inst_name: &String,
    ) -> resolve::Result<Arc<Box<T>>>
        where T: ?Sized + 'static
    {
        Node::instantiate(&self.0, repo_name, inst_name)
    }

    pub fn resolve<R>(&self) -> resolve::Result<R>
        where Self: ResolveRoot<R>
    {
        ResolveRoot::resolve(self)
    }

    // pub fn nested<S>(&self, name: S) -> Builder<Nested>
    //     where S: Into<String>
    // {
    //     Builder::new(Nested::new(&self.0, name.into()))
    // }
}

impl ResolveRoot<Self> for Transient {
    fn resolve(&self) -> resolve::Result<Self> { Ok(Self::rewrap(self.0.clone())) }
}

impl<T, I> ResolveRoot<Instance<T, I>> for Transient
    where T: Reflect + ?Sized, I: Reflect
{
    fn resolve(&self) -> resolve::Result<Instance<T, I>> {
        self.instantiate::<T>(T::name(), I::name())
            .map(|i| Instance::new(i))
    }
}

// for Builder
impl AsRef<Arc<Node>> for Transient {
    fn as_ref(&self) -> &Arc<Node> { &self.0 }
}
