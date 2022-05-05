use std::error::{self, Error as StdError};
use std::fmt;
use std::sync::Arc;
use downcast::{AnySync, TypeMismatch};

pub use anyhow::{Result, Error};

// errors --------------------------------------------------

#[derive(Debug)]
pub struct InstancerNotFoundError {
    pub service_name: String
}

impl InstancerNotFoundError {
    pub fn new(service_name: String) -> Self {
        Self{ service_name }
    }
}

impl fmt::Display for InstancerNotFoundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "instancer for {} not found", self.service_name)
    }
}

impl error::Error for InstancerNotFoundError {}

#[derive(Debug)]
pub struct InstanceCreationError {
    pub service_name: String,
    pub creation_error: Error,
}

impl InstanceCreationError {
    pub fn new(service_name: String, creation_error: Error) -> Self {
        Self{ service_name, creation_error }
    }
}

impl fmt::Display for InstanceCreationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "instance creation for {} failed: {}", self.service_name, self.creation_error)
    }
}

impl error::Error for InstanceCreationError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
       Some(&*self.creation_error)
    }
}

#[derive(Debug)]
pub struct InstanceTypeError {
    pub service_name: String,
    pub type_mismatch: TypeMismatch,
}

impl InstanceTypeError {
    pub fn new(service_name: String, type_mismatch: TypeMismatch) -> Self {
        Self{ service_name, type_mismatch }
    }
}

impl fmt::Display for InstanceTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "wrong type for instance of {}: {}", self.service_name, self.type_mismatch)
    }
}

impl error::Error for InstanceTypeError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
       Some(&self.type_mismatch)
    }
}

// Service --------------------------------------------------

pub trait Service: Send + Sync + 'static {
    fn service_name() -> String {
        std::any::type_name::<Self>()
            .replace("dyn ", "")
            .replace("::", ".")
    }
}

// InstanceRef & TypedInstanceRef --------------------------------------------------

pub type InstanceRef = Arc<dyn AnySync>;

#[allow(type_alias_bounds)]
pub type TypedInstanceRef<S: ?Sized> = Arc<Box<S>>;

pub use TypedInstanceRef as I;
