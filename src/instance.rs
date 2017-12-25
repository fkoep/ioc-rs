use resolve::{ResolveError, ResolveResult, Resolve};
use node::Node;
use downcast::{Any, Downcast};
use std::marker::PhantomData;
use std::sync::Arc;
use std::ops::Deref;

pub trait Reflect: 'static {
    fn name_value() -> String;

    fn name() -> &'static str {
        use intern::Intern;
        use std::any::TypeId;
        use std::collections::HashMap;
        use std::sync::Mutex;

        lazy_static!{
            static ref NAMES: Mutex<HashMap<TypeId, String>> = Default::default();
        }

        NAMES.lock().unwrap()
            .entry(TypeId::of::<Self>())
            .or_insert_with(Self::name_value)
            .intern()
    }
}

#[macro_export]
macro_rules! ioc_reflect {
    // TODO test
    ($name:expr, <$($params:ident),+ $(,)*> $svc:ty $(where $($bounds:tt)+)*) => {
        impl<$($params),+> $crate::Reflect for $svc
            $(where $($bounds)*)*
        {
            fn name_value() -> String { String::from($name) }
        }
    };
    // TODO test
    (<$($params:ident),+ $(,)*> $svc_ident:ident <$($svc_params:ident)+> $(where $($bounds:tt)+)*) => {
        ioc_reflect!({
            let svc_params = [$(Reflect::name($svc_params)),+];
            format!("{}<{}>", stringify!($svc_ident), svc_params.join(","))
        }, <$($params),+> $svc_ident <$($svc_params)+> $(where $($bounds)+)*);
    };
    ($name:expr, $svc:ty) => {
        impl $crate::Reflect for $svc {
            fn name_value() -> String { String::from($name) }
        }
    };
    ($svc:ident) => {
        ioc_reflect!(stringify!($svc), $svc);
    };
}

pub struct DefaultInstance(());

// impl Reflect for DefaultInstance {
//     fn name_value() -> String { "default".into() }
// }
ioc_reflect!("default", DefaultInstance);

pub struct Instance<S, I = DefaultInstance> {
    obj: Arc<Any>,
    _p: PhantomData<(S, I)>,
}

impl<S, I> Instance<S, I> {
    // TODO assert type
    pub fn new(obj: Arc<Any>) -> Self { Self{ obj, _p: PhantomData } }
}

impl<S, I> Clone for Instance<S, I> {
    fn clone(&self) -> Self { Self::new(self.obj.clone()) }
}

impl<S, I> Deref for Instance<S, I> 
    where S: Any
{
    type Target = S;
    fn deref(&self) -> &Self::Target { self.obj.downcast_ref().unwrap() }
}

impl<S, I> Resolve for Instance<S, I>
    where S: Resolve + Reflect, S::Depend: Resolve, I: Reflect
{
    type Depend = Arc<Node>;
    
    fn resolve(node: Self::Depend) -> ResolveResult<Self> {
        match node.instantiate(S::name(), I::name()) {
            Ok((fac_path, inst)) => {
                if let Err(e) = Downcast::<S>::downcast_ref(&*inst) {
                    return Err(ResolveError::InstanceTypeMismatch(fac_path.clone(), I::name().to_owned(), e))
                }
                Ok(Self::new(inst))
            }
            Err(e) => Err(e),
        }
    }
}

