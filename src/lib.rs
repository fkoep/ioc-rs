#![feature(unsize)]
#![feature(macro_vis_matcher)]

extern crate case;
extern crate chashmap;
extern crate downcast;
extern crate intern;
pub extern crate futures;
#[macro_use]
extern crate lazy_static;
extern crate spin;
#[macro_use]
extern crate variadic_generics;

pub mod resolve;
pub mod internal;
pub mod containers;

pub use resolve::*;
pub use containers::*;
pub use containers::Lifecycle::*;
pub use containers::Instance as I;

// /// Implements ioc::Reflect for Interface based on module name.
// ///
// /// TODO Where should this be located?
// #[macro_export]
// macro_rules! ai_ioc {
//     ($module:ident $(<$($ty_generics:ident),+>)* {
//         $(
//             fn $methods:ident($($args:ident: $arg_tys:ty),*) -> $ret_tys:ty;
//         )*
//     }) => {
// impl $(<$($ty_generics),+>)* $crate::Reflect for Interface
// $(<$($ty_generics),+>)*
//             $(where $($ty_generics: $crate::Reflect),+)*
//         {
//             fn name_value() -> String {
// let params: &[&str] = &[$($(&**<$ty_generics as
// $crate::Reflect>::name())+)*];
//                 // TODO convert case for $module
//                 format!("{}<{}>", stringify!($module), params.join(","))
//             }
//         }
//     }
// }

// /// TODO Remove?
// ///
// /// TODO Where should this be located?
// #[macro_export]
// macro_rules! ai_ioc_spawn {
//     ($module:ident $(<$($ty_generics:ident),+>)* {
//         $(
//             fn $methods:ident($($args:ident: $arg_tys:ty),*) -> $ret_tys:ty;
//         )*
//     }) => {
//         pub struct IocSpawn<$($($ty_generics,)+)* _O> {
//             intf: TaskInterface $(<$($ty_generics),+>)*,
//             _p: ::std::marker::PhantomData<fn(_O)>,
//         }

// // impl<$($($ty_generics,)+)* _O> IocSpawn <$($($ty_generics,)+)*
// _O> {
// //     $vis fn new(intf: TaskInterface $(<$($ty_generics),+>)*) ->
// Self {
//         //         use std::marker::PhantomData;
//         //         Self{ intf, _p: PhantomData }
//         //     }
//         // }

// impl<$($($ty_generics,)+)* _O> Interface $(<$($ty_generics),+>)* for
// IocSpawn <$($($ty_generics,)+)* _O>
//              $(where $($ty_generics: Send + Sync + 'static,)+)*
//         {
//             $(
// fn $methods(&self, $($args: $arg_tys),*) ->
// Box<$crate::futures::Future<Item = $ret_tys, Error = ()> + Send> {
//                     self.intf.$methods($($args),*)
//                 }
//             )*
//         }

// impl<$($($ty_generics,)+)* _O> $crate::Resolve for IocSpawn
// <$($($ty_generics,)+)* _O>
//         where
//              $($($ty_generics: Send + Sync + 'static,)+)*
//             _O: ImplementationAsync $(<$($ty_generics),+>)*,
//             $crate::Transient: $crate::ResolveRoot<_O>,
//         {
//             type Dependency = $crate::Transient;
//             fn resolve(trans: Self::Dependency) -> $crate::Result<Self> {
//                 use $crate::futures::sync::oneshot;
//                 use $crate::futures::Future;
//                 use std::thread;

//                 let (res_tx, res_rx) = oneshot::channel();
//                 thread::spawn(move || {
//                     match trans.resolve::<_O>() {
//                         Ok(mut obj) => {
//                             drop(trans);
//                             let (intf, server) = self::task_channel();
//                             let _ = res_tx.send(Ok(intf));
//                             server.serve(&mut obj);
//                         }
//                         Err(err) => {
//                             let _ = res_tx.send(Err(err));
//                         }
//                     }
//                 });
//                 match res_rx.wait() {
// Ok(Ok(intf)) => Ok(Self{ intf, _p:
// ::std::marker::PhantomData }),
//                     Ok(Err(err)) => Err(err),
// Err(_) => panic!() // TODO panicked within spawned
// thread?
//                 }
//             }
//         }
//     }
// }
