use std::error::Error;

// TODO move somewhere else?
pub type GenericError = Box<Error + Send + Sync>;
pub type GenericResult<T> = Result<T, GenericError>;

pub trait Resolve: Sized + 'static {
    type Dep;
    type Err = GenericError;
    fn resolve(dep: Self::Dep) -> Result<Self, Self::Err>;
}

pub trait Container {
    type Err: Error + Send + Sync + 'static;
}

/// Careful when using this trait, or you'll be in for a world of stack
/// overflows.
pub trait ResolveStart<R>: Container {
    /// TODO should be (self)?
    fn resolve_start(&self) -> Result<R, Self::Err>;
}

impl<C> ResolveStart<()> for C
    where C: Container
{
    fn resolve_start(&self) -> Result<(), C::Err> { Ok(()) }
}

impl<R, C> ResolveStart<R> for C
    where R: Resolve, C: ResolveStart<R::Dep>, C::Err: From<R::Err>
{
    fn resolve_start(&self) -> Result<R, C::Err> {
        Ok(R::resolve(<C as ResolveStart<R::Dep>>::resolve_start(self)?)?)
    }
}

va_expand!{ ($va_len:tt) ($($va_idents:ident),+) ($($va_indices:tt),+)
    impl<$($va_idents,)+ C> ResolveStart<($($va_idents,)+)> for C
    where 
        $($va_idents: Resolve,)+
        $(C: ResolveStart<$va_idents::Dep>,)+
        $(C::Err: From<$va_idents::Err>,)+
    {
        fn resolve_start(&self) -> Result<($($va_idents,)+), C::Err> { 
            Ok(($(
                $va_idents::resolve(<C as ResolveStart<$va_idents::Dep>>::resolve_start(self)?)?,
            )+))
        }
    }
}

