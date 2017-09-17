use std::error::Error as StdError;
use std::result::Result as StdResult;

pub type Error = Box<StdError + Send + Sync>;
pub type Result<T> = StdResult<T, Error>;

pub trait Resolve: Sized {
    // TODO? type Output;
    type Dependency;

    fn resolve(dep: Self::Dependency) -> Result<Self>;
}

/// Careful when using this trait, or you'll be in for a world of stack
/// overflows.
///
/// TODO Get rid of 'a?
pub trait ResolveRoot<R> {
    fn resolve(&self) -> Result<R>;
}

impl<R, C> ResolveRoot<R> for C
    where R: Resolve, C: ResolveRoot<R::Dependency>
{
    fn resolve(&self) -> Result<R> {
        R::resolve(<C as ResolveRoot<R::Dependency>>::resolve(self)?)
    }
}

impl<C> ResolveRoot<()> for C {
    fn resolve(&self) -> Result<()> { Ok(()) }
}

va_expand!{ ($va_len:tt) ($($va_idents:ident),+) ($($va_indices:tt),+)
    impl<$($va_idents,)+ C> ResolveRoot<($($va_idents,)+)> for C
    where 
        $($va_idents: Resolve,)+
        $(C: ResolveRoot<$va_idents::Dependency>,)+
    {
        fn resolve(&self) -> Result<($($va_idents,)+)> { 
            Ok(($(
                $va_idents::resolve(<C as ResolveRoot<$va_idents::Dependency>>::resolve(self)?)?,
            )+))
        }
    }
}

// TODO
// mod compile_test {
//     pub struct MyCtx;

//     impl ResolveRoot<&MyCtx> for MyCtx {
//         fn resolve(&self) -> Result<&MyCtx> { Ok(self) }
//     }

//     pub struct MyRes;

//     impl Resolve for MyRes {
//         type Dependency = ();
//         fn resolve(_: Self::Dependency) -> Result<Self> {
//             Ok(MyRes)
//         }
//     }
// }
