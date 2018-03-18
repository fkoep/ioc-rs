#![feature(proc_macro)]

extern crate proc_macro;
extern crate syn;
#[macro_use]
extern crate quote;

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn ioc_reflect(meta: TokenStream, item: TokenStream) -> TokenStream {
    let item_ast = syn::parse_item(&item.to_string()).unwrap();

    let base_name = match syn::parse_outer_attr(&format!("#[ioc {}]", meta)) {
        Ok(syn::Attribute{ value: syn::MetaItem::NameValue(_, syn::Lit::Str(s, _)), ..}) => s,
        Ok(syn::Attribute{ value: syn::MetaItem::Word(_), ..}) => item_ast.ident.to_string(),
        _ => panic!("Expected syntax: #[ioc_reflect = MyType]")
    };

    let impl_gen = impl_reflect(base_name, item_ast.clone());
    quote!(
        #item_ast
        #impl_gen
    ).parse().unwrap()
}

fn impl_reflect(base_name: String, item_ast: syn::Item) -> quote::Tokens {
    let ident = item_ast.ident;
    let mut generics = match item_ast.node {
        syn::ItemKind::Enum(_, g) => g,
        syn::ItemKind::Struct(_, g) => g,
        syn::ItemKind::Trait(_, g, _, _) => g,
        _ => panic!("ioc::Reflect-impl can only be generated for enums, structs and traits.")
    };

    let mut ty_param_idents = Vec::new();
    for ty_param in &mut generics.ty_params {
        ty_param.bounds.push(syn::parse_ty_param_bound("::ioc::Reflect").unwrap());
        ty_param_idents.push(ty_param.ident.clone());
    }
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    if ty_param_idents.is_empty() {
        quote!{
            impl ::ioc::Reflect for #ident {
                fn name_init() -> String { 
                    stringify!(#base_name).to_owned()
                }
            }
        }
    } else {
        quote!{
            impl #impl_generics ::ioc::Reflect for #ident #ty_generics #where_clause {
                fn name_init() -> String {
                    let params: &[&str] = &[#(<#ty_param_idents as ::ioc::Reflect>::name()),*];
                    format!("{}<{}>", #base_name, params.join(","))
                }
            }
        }
    }
}

#[proc_macro_derive(IocResolve)]
pub fn ioc_resolve(item: TokenStream) -> TokenStream {
    let item_ast = syn::parse_item(&item.to_string()).unwrap();

    let impl_gen = impl_resolve(item_ast.clone());
    quote!(
        // #item_ast
        #impl_gen
    ).parse().unwrap()
}

fn impl_resolve(item_ast: syn::Item) -> quote::Tokens {
    let ident = item_ast.ident;
    let (fields, generics) = match item_ast.node {
        syn::ItemKind::Struct(syn::VariantData::Struct(f), g) => (f, g),
        syn::ItemKind::Struct(syn::VariantData::Unit, g) => (vec!(), g),
        _ => panic!("ioc::Resolve-impl can only be generated for non-tuple structs")
    };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let field_idents: Vec<_> = fields.iter().map(|f| f.ident.as_ref().unwrap().clone()).collect();
    let field_idents2 = field_idents.clone();
    let field_tys: Vec<_> = fields.iter().map(|f| f.ty.clone()).collect();

    quote!{
        impl #impl_generics ::ioc::Resolve for #ident #ty_generics #where_clause {
            type Dep = (#(#field_tys,)*);
            type Err = ::ioc::GenericError;
            fn resolve((#(#field_idents,)*): Self::Dep) -> ::std::result::Result<Self, Self::Err> {
                Ok(Self{ #(#field_idents2,)* })
            }
        }
    }
}
