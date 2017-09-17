pub trait Reflect: 'static {
    /// TODO(assoc_static) Replace with associated static.
    fn name_value() -> String;

    /// TODO(assoc_static) Replace with associated static.
    fn name() -> &'static String {
        use intern::Intern;
        use chashmap::CHashMap;
        use std::any::TypeId;

        lazy_static!{
            static ref NAMES: CHashMap<TypeId, &'static String> = Default::default();
        }

        let ty = TypeId::of::<Self>();
        NAMES.upsert(ty, || Self::name_value().intern(), |_| {});
        *NAMES.get(&ty).unwrap()
    }
}

// TODO Naming? `Default`, `DefaultInstance`
pub struct DefaultInstancer;

impl Reflect for DefaultInstancer {
    fn name_value() -> String { "DefaultInstancer".to_owned() }
}

// #[macro_export]
// macro_rules! ioc_reflect {
//     ($ty:ty, $name:expr) => {
//         impl $crate::Reflect for $ty {
//             fn name_value() -> String { ($name).into() }
//         }
//     };
//     ($ident:ident) => {
//         ioc_reflect!($ident, stringify!($ident));
//     };
// }
