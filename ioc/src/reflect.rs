use std::any::TypeId;
use std::collections::HashMap;
use std::sync::Mutex;

pub trait Reflect: 'static {
    fn name_init() -> String;

    fn name() -> &'static str {
        lazy_static!{
            static ref NAMES: Mutex<HashMap<TypeId, String>> = Default::default();
        }

        let ptr = NAMES.lock().unwrap()
            .entry(TypeId::of::<Self>())
            .or_insert_with(Self::name_init).as_str() as *const str;
        unsafe { &*ptr }
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

