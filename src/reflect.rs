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
