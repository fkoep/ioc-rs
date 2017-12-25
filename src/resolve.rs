// use node::Path;
use downcast::TypeMismatch;
use std::error::Error;
use std::fmt;

// pub type GenericError = Box<Error + Send + Sync>;
// pub type GenericResult<T> = Result<T, GenericError>;

// // TODO Move into node.rs? Naming? InstantiationError?
// #[derive(Debug)]
// pub enum ResolveError {
//     // TODO Naming: FactoryNotFound?
//     InstanceNotFound(String, String),
//     InstanceTypeMismatch(Path, String, TypeMismatch),
//     // DependencyError(Path, String, Box<ResolveError>),
//     Other(GenericError),
// }

// impl From<GenericError> for ResolveError {
//     fn from(e: GenericError) -> Self { ResolveError::Other(e) }
// }

// impl fmt::Display for ResolveError {
//     fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
//         Ok(()) // TODO
//     }
// }

// impl Error for ResolveError {
//     fn description(&self) -> &str { "" } // TODO
// }

// pub type ResolveResult<T> = Result<T, ResolveError>;

pub trait Resolve: Sized {
    type Depend;
    type Error;
    fn resolve(dep: Self::Depend) -> Result<Self, Self::Error>;
}

pub trait Resolver {
    type Error;
}

/// Careful when using this trait, or you'll be in for a world of stack
/// overflows.
///
/// TODO Naming?
pub trait ResolveStart<R>: Resolver {
    fn resolve_start(&self) -> Result<R, Self::Error>;
}

// FIXME
// impl<C> ResolveStart<C> for C
//     where C: Resolver + Clone
// {
//     fn resolve_start(&self) -> Result<C, Self::Error> { self.clone() }
// }

impl<R, C> ResolveStart<R> for C
    where R: Resolve, C: ResolveStart<R::Depend>, C::Error: From<R::Error>
{
    fn resolve_start(&self) -> Result<R, Self::Error> {
        Ok(R::resolve(<C as ResolveStart<R::Depend>>::resolve_start(self)?)?)
    }
}

va_expand!{ ($va_len:tt) ($($va_idents:ident),+) ($($va_indices:tt),+)
    impl<$($va_idents,)+ C> ResolveStart<($($va_idents,)+)> for C
    where 
        $($va_idents: Resolve,)+
        $(C: ResolveStart<$va_idents::Depend>,)+
        $(C::Error: From<$va_idents::Error>,)+
    {
        fn resolve_start(&self) -> Result<($($va_idents,)+), C::Error> { 
            Ok(($(
                $va_idents::resolve(<C as ResolveStart<$va_idents::Depend>>::resolve_start(self)?)?,
            )+))
        }
    }
}
