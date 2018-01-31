#[macro_use]
pub extern crate ioc;
extern crate serde_value;
#[cfg(feature = "ron")]
extern crate ron;
#[cfg(feature = "toml")]
extern crate toml;

pub use serde_value::{Value, to_value, SerializerError, DeserializerError};

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::fmt::Debug;
use std::io::{Read, Write};
use std::fs;

// ++++++++++++++++++++ Configuration ++++++++++++++++++++

pub trait Configuration: ioc::Instance {
    fn has(&self, path: &str) -> bool;
    fn get(&self, path: &str) -> Option<Value>;
    fn make_path(&self, path: &str) -> Result<(), ()>;
    fn set(&self, path: &str, val: Value) -> Result<(), ()>;

    fn root(&self) -> Value {
        // TODO should unwrap?
        self.get("").unwrap()
    }
}

ioc_impl_reflect!(Configuration);

// ++++++++++++++++++++ ConfigMap ++++++++++++++++++++

pub fn split_first_key(path: &str) -> (&str, &str) {
    assert_ne!(path, "");
    path.split_at(path.find('.').unwrap_or(path.len()))
}

pub fn split_last_key(path: &str) -> (&str, &str) {
    assert_ne!(path, "");
    path.split_at(path.rfind('.').unwrap_or(0))
}

pub fn value_get<'a>(val: &'a Value, path: &str) -> Option<&'a Value> {
    if path == "" { return Some(val) }
    let (key, rest) = split_first_key(path);
    match *val {
        Value::Map(ref m) => {
             /* FIXME unneeded alloc */
            let key = &Value::String(key.to_owned());
            m.get(key).and_then(|sub| value_get(sub, rest))
        }
        _ => None
    }
}

pub fn value_make_path<'a>(val: &'a mut Value, path: &str) -> Result<&'a mut BTreeMap<Value, Value>, ()> {
    match *val {
        Value::Map(ref mut m) => {
            if path == "" { return Ok(m) }
            let (key, rest) = split_first_key(path);
             /* FIXME unneeded alloc */
            let key = Value::String(key.to_owned());
            let e = m.entry(key)
                .or_insert_with(|| Value::Map(BTreeMap::new()));
            value_make_path(e, rest)
        }
        _ => Err(())
    }
}

pub fn value_set(val: &mut Value, path: &str, new_val: Value) -> Result<(), ()>{
    if path == "" { *val = new_val; return Ok(()) }
    let (rest, key) = split_last_key(path);
    value_make_path(val, rest)
        .map(move |m| { m.insert(Value::String(key.to_owned()), new_val); })
}

/// TODO Naming?
pub struct ConfigMap(RwLock<Value>);

impl ConfigMap {
    fn _new(val: Value) -> Self { ConfigMap(RwLock::new(val)) }

    // TODO
    // pub fn new(sections: BTreeMap<String, Value>) -> Self {
    //     Self::_new(Value::Map(sections.into_iter().map(|(k, v)| (Value::String(k), v)).collect()))
    // }
}

impl Default for ConfigMap {
    fn default() -> Self { Self::_new(Value::Map(Default::default())) }
}

impl Configuration for ConfigMap {
    fn has(&self, path: &str) -> bool {
        let val = self.0.read().unwrap();
        value_get(&val, path).is_some()
    }
    fn get(&self, path: &str) -> Option<Value> {
        let val = self.0.read().unwrap();
        value_get(&val, path).map(Clone::clone)
    }
    fn make_path(&self, path: &str) -> Result<(), ()> {
        let mut val = self.0.write().unwrap();
        value_make_path(&mut val, path).and(Ok(()))
    }
    fn set(&self, path: &str, new_val: Value) -> Result<(), ()> {
        let mut val = self.0.write().unwrap();
        value_set(&mut val, path, new_val)
    }
}

pub trait ConfigStruct: Sized {
    fn set_defaults(cfg: &Configuration, path: &str) -> Result<(), ()>;
    fn load(cfg: &Configuration, path: &str) -> Result<Self, DeserializerError>;
}

#[macro_export]
macro_rules! ioc_config {
    (@default) => { Default::default() };
    (@default $expr:expr) => { $expr };

    ($(
        $(#[$sec_meta:meta])*
        section $sec:ident { 
            $(
                $(#[$opt_meta:meta])*
                opt $opt:ident $(= $default:expr)*
            ),*
            $(,)*
        }
    )*) => {$(
        $(#[$sec_meta])*
        #[derive(Serialize, Deserialize)]
        #[serde(default)]
        pub struct $sec {
            $(
                $(#[$opt_meta])*
                pub $opt,
            )*
        }

        impl $crate::ioc::Reflect for $sec {
            fn name_init() -> String { stringify!($sec).to_owned() }
        }

        impl Default for $sec {
            fn default() -> Self {
                $(
                    let $opt = ioc_config!(@default $($default)*);
                )*
                Self{ $($opt),* }
            }
        }

        impl $crate::ConfigStruct for $sec {
            fn set_defaults(cfg: &$crate::Configuration, path: &str) -> Result<(), ()> {
                cfg.set_path(<$sec as $crate::ioc::Reflect>::name(), $crate::to_value(self).unwrap())
            }
            fn load(cfg: &$crate::Configuration, path: &str) -> Result<Self, $crate::DeserializerError> {
                cfg.get(path)
                    .unwrap_or(Value::Map(BTreeMap::new()))
                    .deserialize_into()
            }
        }

        impl $crate::ioc::Resolve for $sec {
            type Depend = $crate::ioc::I<$crate::Configuration>;
            type Error = $crate::DeserializeError;
            fn resolve(cfg: Self::Depend) -> Result<Self, Self::Error> {
                Self::load(cfg, Self::name())
            }
        }
    )*};
}

// ++++++++++++++++++++ ConfigFormat ++++++++++++++++++++

pub trait ConfigFormat {
    fn load_str(&self, s: &str) -> ioc::GenericResult<ConfigMap>;

    fn load_file(&self, path: &str) -> ioc::GenericResult<ConfigMap> {
        let ref mut buf = String::new();
        let mut file = fs::File::open(path)?;
        file.read_to_string(buf)?;
        drop(file);
        self.load_str(buf)
    }

    fn save_string(&self, cfg: &Configuration) -> ioc::GenericResult<String>;

    fn save_file(&self, path: &str, cfg: &Configuration) -> ioc::GenericResult<()> {
        let s = self.save_string(cfg)?;
        let mut file = fs::File::open(path)?;
        file.write_all(s.as_ref())?;
        Ok(())
    }
}

// ++++++++++++++++++++ ConfigFormat impls ++++++++++++++++++++

// TODO expose PrettyConfig
#[cfg(feature = "ron")]
pub struct RonConfigFormat;

#[cfg(feature = "ron")]
impl ConfigFormat for RonConfigFormat {
    fn load_str(&self, s: &str) -> ioc::GenericResult<ConfigMap> {
        Ok(ConfigMap::_new(ron::de::from_str::<Value>(s)?))
    }
    fn save_string(&self, cfg: &Configuration) -> ioc::GenericResult<String> {
        let s = ron::ser::to_string_pretty(&cfg.root(), Default::default())?;
        Ok(s)
    }
}

// TODO expose PrettyConfig
#[cfg(feature = "toml")]
pub struct TomlConfigFormat;

#[cfg(feature = "toml")]
impl ConfigFormat for TomlConfigFormat {
    fn load_str(&self, s: &str) -> ioc::GenericResult<ConfigMap> {
        Ok(ConfigMap::_new(toml::de::from_str::<Value>(s)?))
    }
    fn save_string(&self, cfg: &Configuration) -> ioc::GenericResult<String> {
        let s = toml::ser::to_string_pretty(&cfg.root())?;
        Ok(s)
    }
}
