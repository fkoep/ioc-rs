#[macro_use]
extern crate ioc;

use ioc::Any;

pub trait MyService: Any + Send + Sync {
    fn foo(&self);
}

ioc_reflect!(MyService);

pub struct MyImpl;

impl MyService for MyImpl {
    fn foo(&self){ print!("MyImpl") }
}

impl ioc::Resolve for MyImpl {
    type Depend = ();
    type Error = ioc::Error;
    fn resolve(_: Self::Depend) -> ioc::Result<Self> { Ok(MyImpl) }
}

fn main(){
    let comp = ioc::root()
        .with_default::<MyService, MyImpl, ioc::Default>()
        .with_cache::<ioc::Default>();

    comp.resolve::<ioc::I<MyService>>().unwrap().foo();
}
