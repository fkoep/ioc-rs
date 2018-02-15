use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Mutex;
use std::mem;

pub trait Reflect: 'static {
    fn name_init() -> String;

    fn name() -> &'static str {
        lazy_static!{
            static ref NAMES: Mutex<HashMap<TypeId, String>> = Default::default();
        }

        let ty = TypeId::of::<Self>();
        if !NAMES.lock().unwrap().contains_key(&ty) {
            let name = Self::name_init();
            NAMES.lock().unwrap().insert(ty, name);
        }
        unsafe { 
            mem::transmute::<&str, &'static str>(NAMES.lock().unwrap().get(&ty).unwrap().as_str())
        }
    }
}

#[macro_export]
macro_rules! ioc_impl_reflect {
    (<$($params:ident),+ $(,)*> $svc_ident:ident <$($svc_params:ident),+> $(where $($bounds:tt)+)*) => {
        ioc_impl_reflect!({
            let svc_params = [$(<$svc_params as $crate::Reflect>::name()),+];
            format!("{}<{}>", stringify!($svc_ident), svc_params.join(","))
        }, <$($params),+> $svc_ident <$($svc_params),+> $(where $($bounds)+)*);
    };
    ($name:expr, <$($params:ident),+ $(,)*> $svc:ty $(where $($bounds:tt)+)*) => {
        impl<$($params),+> $crate::Reflect for $svc
            where $($params: $crate::Reflect,)* $($($bounds)*)*
        {
            fn name_init() -> String { String::from($name) }
        }
    };
    ($svc:ident) => {
        ioc_impl_reflect!(stringify!($svc), $svc);
    };
    ($name:expr, $svc:ty) => {
        impl $crate::Reflect for $svc {
            fn name_init() -> String { String::from($name) }
        }
    };
}

