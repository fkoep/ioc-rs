#![feature(proc_macro)]

extern crate ioc;
extern crate ioc_m;

use ioc_m::ioc_reflect;

#[ioc_reflect]
trait MyInterface: Send + Sync {}

struct MySingleton;

impl MyInterface for MySingleton {}

impl ioc::Resolve for MySingleton {
    type Dependency = ();
    fn resolve(_: Self::Dependency) -> ioc::Result<Self> {
        println!("- Singleton");
        Ok(MySingleton)
    }
}

struct MyPerRequest;

impl MyInterface for MyPerRequest {}

impl ioc::Resolve for MyPerRequest {
    type Dependency = ();
    fn resolve(_: Self::Dependency) -> ioc::Result<Self> {
        println!("- PerRequest");
        Ok(MyPerRequest)
    }
}

struct MyAlwaysUnique;

impl MyInterface for MyAlwaysUnique {}

impl ioc::Resolve for MyAlwaysUnique {
    type Dependency = ioc::I<MyInterface, InstanceA>;
    fn resolve(_: Self::Dependency) -> ioc::Result<Self> {
        println!("- AlwaysUnique");
        Ok(MyAlwaysUnique)
    }
}

#[ioc_reflect]
struct InstanceA;
#[ioc_reflect]
struct InstanceB;
#[ioc_reflect]
struct InstanceC;

fn main() {
    let root = ioc::Builder::root("Root")
        .repository::<MyInterface>()
        .set::<InstanceA, MySingleton>(ioc::Singleton)
        .set::<InstanceB, MyPerRequest>(ioc::PerRequest)
        .set::<InstanceC, MyAlwaysUnique>(ioc::AlwaysUnique)
        .exit()
        .build();

    println!("ROOT I");
    root.resolve::<ioc::I<MyInterface, InstanceA>>().unwrap();
    root.resolve::<ioc::I<MyInterface, InstanceB>>().unwrap();
    root.resolve::<ioc::I<MyInterface, InstanceC>>().unwrap();

    println!("ROOT II");
    root.resolve::<ioc::I<MyInterface, InstanceA>>().unwrap();
    root.resolve::<ioc::I<MyInterface, InstanceB>>().unwrap();
    root.resolve::<ioc::I<MyInterface, InstanceC>>().unwrap();

    root.request(|t| {
            let t = t.build();

            println!("TRANSIENT I");
            t.resolve::<ioc::I<MyInterface, InstanceA>>().unwrap();
            t.resolve::<ioc::I<MyInterface, InstanceB>>().unwrap();
            t.resolve::<ioc::I<MyInterface, InstanceC>>().unwrap();

            println!("TRANSIENT II");
            t.resolve::<ioc::I<MyInterface, InstanceA>>().unwrap();
            t.resolve::<ioc::I<MyInterface, InstanceB>>().unwrap();
            t.resolve::<ioc::I<MyInterface, InstanceC>>().unwrap();

            // let nested = t.nested("Nested").build();

            // println!("NESTED I");
            // nested.resolve::<ioc::I<MyInterface, InstanceA>>().unwrap();
            // nested.resolve::<ioc::I<MyInterface, InstanceB>>().unwrap();
            // nested.resolve::<ioc::I<MyInterface, InstanceC>>().unwrap();

            // println!("NESTED II");
            // nested.resolve::<ioc::I<MyInterface, InstanceA>>().unwrap();
            // nested.resolve::<ioc::I<MyInterface, InstanceB>>().unwrap();
            // nested.resolve::<ioc::I<MyInterface, InstanceC>>().unwrap();

            // nested.request(|t| {
            //     let t = t.build();

            //     println!("NESTED-TRANSIENT I");
            //     t.resolve::<ioc::I<MyInterface, InstanceA>>().unwrap();
            //     t.resolve::<ioc::I<MyInterface, InstanceB>>().unwrap();
            //     t.resolve::<ioc::I<MyInterface, InstanceC>>().unwrap();

            //     println!("NESTED-TRANSIENT II");
            //     t.resolve::<ioc::I<MyInterface, InstanceA>>().unwrap();
            //     t.resolve::<ioc::I<MyInterface, InstanceB>>().unwrap();
            //     t.resolve::<ioc::I<MyInterface, InstanceC>>().unwrap();

            //     Ok(())
            // }).unwrap();

            Ok(())
        })
        .unwrap();

    // output:
    //
    // ROOT I
    // - Singleton
    // - PerRequest
    // - AlwaysUnique
    // ROOT II
    // - PerRequest
    // - AlwaysUnique
    // TRANSIENT I
    // - PerRequest
    // - AlwaysUnique
    // TRANSIENT II
    // - AlwaysUnique
    // NESTED I
    // - AlwaysUnique
    // NESTED II
    // - AlwaysUnique
    // NESTED-TRANSIENT I
    // - AlwaysUnique
    // NESTED-TRANSIENT II
    // - AlwaysUnique
}
