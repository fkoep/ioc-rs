use std::error::Error;

pub trait Resolve: Sized + 'static {
    type Depend;
    type Error;
    fn resolve(dep: Self::Depend) -> Result<Self, Self::Error>;
}

pub trait Container {
    type Error: Error + Send + Sync + 'static;
}

/// Careful when using this trait, or you'll be in for a world of stack
/// overflows.
pub trait ResolveStart<R>: Container {
    /// TODO should be (self)?
    fn resolve_start(&self) -> Result<R, Self::Error>;
}

impl<C> ResolveStart<()> for C
    where C: Container
{
    fn resolve_start(&self) -> Result<(), C::Error> { Ok(()) }
}

impl<R, C> ResolveStart<R> for C
    where R: Resolve, C: ResolveStart<R::Depend>, C::Error: From<R::Error>
{
    fn resolve_start(&self) -> Result<R, C::Error> {
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

