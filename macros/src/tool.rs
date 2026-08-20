// SPDX-License-Identifier: MIT
use crate::casing::Casing;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{Ident, ItemFn, parse_macro_input};

pub(crate) fn tool(attr: TokenStream, item: TokenStream) -> TokenStream {
  let func = parse_macro_input!(item as ItemFn);
  let casing = match parse_casing_attr(attr) {
    Ok(c) => c,
    Err(e) => return e.to_compile_error().into(),
  };
  let casing = casing.as_str();
  let casing = quote! { casing = #casing };

  let fn_name = &func.sig.ident.to_string();
  let struct_ident = Ident::new(&Casing::Pascal.recase(&fn_name), Span::call_site());
  let vis = &func.vis;

  quote! {
    #[derive(Debug, Clone, Copy)]
    #vis struct #struct_ident;

    #[::kiruklaw_agent_loop::macros::toolset(readonly, #casing)]
    impl #struct_ident {
      #func
    }
  }
  .into()
}

fn parse_casing_attr(attr: TokenStream) -> Result<Casing, syn::Error> {
  if attr.is_empty() {
    return Ok(Casing::Snake);
  }
  syn::parse(attr.clone())
}
