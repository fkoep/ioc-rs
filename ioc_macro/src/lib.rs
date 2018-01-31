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
