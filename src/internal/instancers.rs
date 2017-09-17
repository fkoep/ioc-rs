use super::node::Node;
use resolve;
use spin;
use std::cell::Cell;
use std::sync::Arc;

// TODO
// type InstFn = Fn(&String, &String, &Arc<Node>) -> resolve::Result<Box<T>> +
// Send + Sync

// ++++++++++++++++++++ Instancer ++++++++++++++++++++

pub trait Instancer: Send + Sync + 'static {
    type Object: ?Sized + 'static;

    fn instantiate(
        &self,
        repo_name: &String,
        inst_name: &String,
        calling_node: &Arc<Node>,
        //TODO depth: usize,
    ) -> resolve::Result<Arc<Box<Self::Object>>>;
}

// ++++++++++++++++++++ State ++++++++++++++++++++

// TODO
// pub enum State<T> {
//     ThreadLocal(thread_local::ThreadLocal<T>),
//     Once(spin::Once<T>),
// }

// ++++++++++++++++++++ Lazy ++++++++++++++++++++

struct Lazy<T> {
    once: spin::Once<Option<T>>,
}

impl<T> Lazy<T> {
    fn new() -> Self { Self { once: spin::Once::new() } }

    fn get<F>(&self, f: F) -> &T
        where F: FnOnce() -> T
    {
        match *self.once.call_once(|| Some(f())) {
            Some(ref r) => r,
            _ => unreachable!(),
        }
    }

    fn try_get<E, F>(&self, f: F) -> Result<&T, E>
        where F: FnOnce() -> Result<T, E>
    {
        let mut err = None;
        let res = self.once
            .call_once(|| match f() {
                           Ok(r) => Some(r),
                           Err(e) => {
                err = Some(e);
                None
            },
                       });
        match *res {
            Some(ref r) => Ok(r),
            None => Err(err.unwrap()),
        }
    }
}

// ++++++++++++++++++++ Singleton ++++++++++++++++++++

pub struct Singleton<T>
    where T: ?Sized
{
    create_fn: Box<Fn(&String, &String, &Arc<Node>) -> resolve::Result<Box<T>> + Send + Sync>,
    lazy: Lazy<Arc<Box<T>>>,
}

impl<T> Singleton<T>
    where T: ?Sized
{
    pub fn new<F>(f: F) -> Self
        where F: Fn(&String, &String, &Arc<Node>) -> resolve::Result<Box<T>> + Send + Sync + 'static
    {
        Self {
            create_fn: Box::new(f),
            lazy: Lazy::new(),
        }
    }
    pub fn object(obj: Box<T>) -> Self {
        let lazy = Lazy::new();
        lazy.get(|| Arc::new(obj));
        Self {
            create_fn: Box::new(|_, _, _| unreachable!()),
            lazy,
        }
    }
}

impl<T> Instancer for Singleton<T>
    where T: Send + Sync + ?Sized + 'static
{
    type Object = T;

    fn instantiate(
        &self,
        repo_name: &String,
        inst_name: &String,
        calling_node: &Arc<Node>,
    ) -> resolve::Result<Arc<Box<Self::Object>>> {
        assert!(calling_node.is_transient());

        self.lazy
            .try_get(|| {
                (self.create_fn)(repo_name, inst_name, calling_node).map(|o| Arc::new(o))
            })
            .map(|i| i.clone())
    }
}

// ++++++++++++++++++++ PerRequest ++++++++++++++++++++

pub struct PerRequest<T>
    where T: ?Sized
{
    create_fn: Arc<Fn(&String, &String, &Arc<Node>) -> resolve::Result<Box<T>> + Send + Sync>,
    // Only used in case we get inserted in a transient node.
    lazy: Lazy<Arc<Box<T>>>,
}

impl<T> PerRequest<T>
    where T: ?Sized
{
    pub fn new<F>(f: F) -> Self
        where F: Fn(&String, &String, &Arc<Node>) -> resolve::Result<Box<T>> + Send + Sync + 'static
    {
        Self {
            create_fn: Arc::new(f),
            lazy: Lazy::new(),
        }
    }
}

impl<T> Instancer for PerRequest<T>
    where T: Send + Sync + ?Sized + 'static
{
    type Object = T;

    fn instantiate(
        &self,
        repo_name: &String,
        inst_name: &String,
        calling_node: &Arc<Node>,
    ) -> resolve::Result<Arc<Box<Self::Object>>> {
        assert!(calling_node.is_transient());

        /* TODO Confusing and messy! Simplify! */

        let mut transient = calling_node;
        let mut persistent = transient.parent().unwrap();
        loop {
            if !(transient.is_transient() && persistent.is_persistent()) {
                // TODO repeated at the bottom, do-while-loop?

                transient = persistent;
                persistent = persistent.parent().unwrap(); // TODO errmsg out-of-hierarchy/invalid hierarchy
                continue;
            }

            if let Some(inst) = transient.get_instancer(repo_name, inst_name, |i| i.clone()) {
                let found_self = &*inst as *const Instancer<Object = Self::Object> == &*self;
                if found_self {
                    // We got inserted in a transient node. Proceed to act like Singleton.
                    //
                    // TODO in this case, we don't need to clone the instance
                    // TODO Should we panic/return error instead?
                    return self.lazy
                               .try_get(|| {
                        (self.create_fn)(repo_name, inst_name, calling_node).map(|o| {
                            Arc::new(o)
                        })
                    })
                               .map(|i| i.clone());
                } else {
                    // Race condition: Offspring instancer already got injected.
                    // Or: possibly out-of-hierarchy (no way to test this).
                    return inst.instantiate(repo_name, inst_name, calling_node);
                }
            }

            // TODO replace with Node::has_instancer?
            let found_self = persistent.get_instancer(repo_name, inst_name, |inst| {
                &**inst as *const Instancer<Object = Self::Object> == &*self
            });
            match found_self {
                // Found self in persistent parent node. Inject offspring into transient
                // transient node.
                Some(true) => {
                    let invoke: Cell<Option<Arc<Instancer<Object = _>>>> = Cell::new(None);
                    transient.upsert_instancer(repo_name,
                                               inst_name,
                                               || {
                        let f = self.create_fn.clone();
                        let insert = Arc::new(Singleton::new(move |r, i, n| f(r, i, n)));
                        invoke.set(Some(insert.clone()));
                        insert
                    },
                                               |inst| invoke.set(Some(inst.clone())));
                    let invoke = invoke.into_inner().unwrap();
                    return invoke.instantiate(repo_name, inst_name, calling_node);
                },
                // Found something, but it's not us!
                Some(false) => panic!(), // TODO errmsg out-of-hiearchy

                // Not contained in `persistent`. Continue.
                None => {},
            }

            transient = persistent;
            persistent = persistent.parent().unwrap(); // TODO errmsg invalid hierarchy
        }
    }
}

// ++++++++++++++++++++ ContainerLocal ++++++++++++++++++++

// TODO
pub struct ContainerLocal;

// ++++++++++++++++++++ AlwaysUnique ++++++++++++++++++++

pub struct AlwaysUnique<T>
    where T: ?Sized
{
    create_fn: Box<Fn(&String, &String, &Arc<Node>) -> resolve::Result<Box<T>> + Send + Sync>,
}

impl<T> AlwaysUnique<T>
    where T: ?Sized
{
    pub fn new<F>(f: F) -> Self
        where F: Fn(&String, &String, &Arc<Node>) -> resolve::Result<Box<T>> + Send + Sync + 'static
    {
        Self { create_fn: Box::new(f) }
    }
}

impl<T> Instancer for AlwaysUnique<T>
    where T: Send + Sync + ?Sized + 'static
{
    type Object = T;

    fn instantiate(
        &self,
        repo_name: &String,
        inst_name: &String,
        calling_node: &Arc<Node>,
    ) -> resolve::Result<Arc<Box<Self::Object>>> {
        assert!(calling_node.is_transient());

        (self.create_fn)(repo_name, inst_name, calling_node).map(|o| Arc::new(o))
    }
}

// TODO ContainerLocal
