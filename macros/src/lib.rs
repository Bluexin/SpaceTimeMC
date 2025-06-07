use proc_macro::TokenStream;
use syn::__private::quote::quote;

#[proc_macro_attribute]
pub fn packet(input: TokenStream, item: TokenStream) -> TokenStream {
    let ast: syn::DeriveInput = syn::parse(item.clone()).unwrap();
    let name = &ast.ident;
    let (impl_generics, ty_generics, _) = ast.generics.split_for_impl();

    let input: proc_macro2::TokenStream = input.into();
    let item: proc_macro2::TokenStream = item.into();

    let code = quote! {
        #item
        impl #impl_generics pumpkin_protocol::ser::packet::Packet for #name #ty_generics {
            const PACKET_ID: i32 = #input;
        }
    };

    code.into()
}
