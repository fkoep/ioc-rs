use super::instancers::Instancer;
use super::path::{Path, PathComponent};
use resolve;
use downcast::{Any, Downcast};
use chashmap::CHashMap;
use std::sync::Arc;

type Repository<T: ?Sized> = CHashMap<String, Arc<Instancer<Object = T>>>;
type RepositoryAny = Any + Send + Sync;

pub struct Node {
    parent: Option<Arc<Node>>,
    path: Path,
    repos: CHashMap<String, Box<RepositoryAny>>,
}

impl Node {
    fn new(parent: Option<Arc<Node>>, path: Path) -> Self {
        Self {
            parent,
            path,
            repos: Default::default(),
        }
    }
    /// Create root node.
    pub fn root(name: String) -> Self { Self::new(None, Path::new(name)) }
    /// Create persistent child-node.
    pub fn persistent_child(parent: Arc<Node>, name: String) -> Self {
        let mut path = parent.path().clone();
        path.push(PathComponent::Persistent(name));
        Self::new(Some(parent), path)
    }
    /// Create transient child-node.
    pub fn transient_child(parent: Arc<Node>, name: Option<String>) -> Self {
        assert!(parent.is_persistent());

        let mut path = parent.path().clone();
        path.push(PathComponent::Transient(name));
        Self::new(Some(parent), path)
    }

    pub fn parent(&self) -> Option<&Arc<Self>> { self.parent.as_ref() }

    pub fn path(&self) -> &Path { &self.path }
    pub fn is_root(&self) -> bool { self.path.is_persistent() }
    pub fn is_persistent(&self) -> bool { self.path.is_persistent() }
    pub fn is_transient(&self) -> bool { self.path.is_transient() }

    // TODO return error?
    fn get_repository<T, R, F>(&self, repo_name: &String, get: F) -> Option<R>
        where T: ?Sized + 'static, F: FnOnce(&Repository<T>) -> R
    {
        let repo_any = match self.repos.get(repo_name) {
            Some(r) => r,
            None => return None,
        };
        let repo: &Repository<T> = (**repo_any).downcast_ref().unwrap();
        Some(get(repo))
    }

    /// Updates or inserts an instancer if contained in the current node.
    pub fn upsert_instancer<T, F, G>(
        &self,
        repo_name: &String,
        inst_name: &String,
        insert: F,
        update: G,
    )
        where T: ?Sized + 'static,
            F: FnOnce() -> Arc<Instancer<Object = T>>,
            G: FnOnce(&mut Arc<Instancer<Object = T>>)
    {
        self.repos
            .upsert(repo_name.clone(), || Box::new(Repository::<T>::new()), |_| {});

        self.get_repository(repo_name,
                            |repo| { repo.upsert(inst_name.clone(), insert, update); });
    }

    /// Gets an instancer if contained in the current node.
    // TODO return error?
    pub fn get_instancer<T, R, F>(
        &self,
        repo_name: &String,
        inst_name: &String,
        get: F,
    ) -> Option<R>
        where T: ?Sized + 'static, F: FnOnce(&Arc<Instancer<Object = T>>) -> R
    {
        self.get_repository(repo_name, |repo| {
                let inst = match repo.get(inst_name) {
                    Some(r) => r,
                    None => return None,
                };
                Some(get(&*inst))
            })
            .and_then(|i| i)
    }

    /// Searches the hierarchy (from self to root) for an instancer and invokes
    /// it if
    /// found. Returns descriptive not-found-error otherwise.
    pub fn instantiate<T>(
        this: &Arc<Self>,
        repo_name: &String,
        inst_name: &String,
    ) -> resolve::Result<Arc<Box<T>>>
        where T: ?Sized + 'static
    {
        let mut next = Some(this);
        while let Some(node) = next {
            let res = node.get_instancer(repo_name, inst_name, |i| i.clone())
                .map(|inst| inst.instantiate(repo_name, inst_name, this));

            if let Some(res) = res {
                return res;
            }

            next = node.parent();
        }
        // TODO proper error type
        Err(format!("Failed to find instancer '{}' in repository '{}' in container '{:?}'!",
                    inst_name,
                    repo_name,
                    this.path())
                    .into())
    }
}
