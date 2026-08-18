// SPDX-License-Identifier: MIT
use crate::casing::Casing;
use crate::tool::{extract_doc_lines, get_arg_type, parse_arg_descriptions, parse_casing_attr};

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{parse_macro_input, FnArg, Ident, ImplItem, ItemImpl, Type};

pub(crate) fn toolset(attr: TokenStream, item: TokenStream) -> TokenStream {
  let impl_block = parse_macro_input!(item as ItemImpl);
  let casing = match parse_casing_attr(&attr) {
    Ok(c) => c,
    Err(e) => return e.to_compile_error().into(),
  };

  if !impl_block.generics.params.is_empty() {
    return syn::Error::new_spanned(
      &impl_block.generics,
      "#[toolset] does not support generic impl blocks",
    )
    .to_compile_error()
    .into();
  }

  let struct_ident = match &*impl_block.self_ty {
    Type::Path(tp) => tp.path.segments.last().expect("empty path").ident.clone(),
    _ => {
      return syn::Error::new_spanned(
        &impl_block.self_ty,
        "#[toolset] can only be applied to impl blocks for named types",
      )
      .to_compile_error()
      .into()
    }
  };

  let struct_name_str = struct_ident.to_string();
  let namespace = Casing::Snake.recase(&struct_name_str);

  let mut tool_impls = Vec::new();
  let mut tool_boxes = Vec::new();

  for impl_item in &impl_block.items {
    let ImplItem::Fn(method) = impl_item else {
      continue;
    };

    let has_self_receiver = matches!(method.sig.inputs.first(), Some(FnArg::Receiver(_)));
    if !has_self_receiver {
      continue;
    }

    let method_name = &method.sig.ident;
    let method_name_str = method_name.to_string();
    let tool_name = format!("{}::{}", namespace, casing.recase(&method_name_str));
    let wrapper_name_str = format!(
      "{}{}",
      struct_name_str,
      Casing::Pascal.recase(&method_name_str)
    );
    let wrapper_ident = Ident::new(&wrapper_name_str, Span::call_site());
    let args_ident = Ident::new(&format!("{}Args", wrapper_name_str), Span::call_site());

    let raw_docs = extract_doc_lines(&method.attrs);
    let arg_descs = parse_arg_descriptions(&raw_docs);
    let tool_desc = raw_docs
      .iter()
      .filter(|line| !line.starts_with('@'))
      .cloned()
      .collect::<Vec<_>>()
      .join("\n");

    let mut arg_entries = Vec::new();
    let mut args_fields = Vec::new();
    let mut call_args = Vec::new();

    for arg in method.sig.inputs.iter().skip(1) {
      if let FnArg::Receiver(r) = arg {
        return syn::Error::new_spanned(r, "unexpected second receiver")
          .to_compile_error()
          .into();
      }
      let FnArg::Typed(pt) = arg else {
        continue;
      };
      let syn::Pat::Ident(ident) = &*pt.pat else {
        continue;
      };
      let name = &ident.ident;
      let name_str = name.to_string();
      let llm_name = casing.recase(&name_str);
      let ty = &pt.ty;

      let (type_tokens, required) = match get_arg_type(ty) {
        Ok(v) => v,
        Err(e) => {
          return syn::Error::new_spanned(
            ty,
            format!("unsupported argument type for #[toolset]: {e}"),
          )
          .to_compile_error()
          .into()
        }
      };

      let mut builder = quote! {
        ::kiruklaw_agent_loop::tools::AgentToolArg::new(#llm_name, #type_tokens)
      };
      if let Some(desc) = arg_descs.get(&name_str) {
        builder = quote! { #builder.with_description(#desc.to_string()) };
      }
      if required {
        builder = quote! { #builder.required() };
      }

      arg_entries.push(builder);
      args_fields.push(quote! { #name: #ty });
      call_args.push(quote! { args.#name });
    }

    let serde_rename = match &casing {
      Casing::Snake => None,
      Casing::Camel => Some("camelCase"),
      Casing::Kebab => Some("kebab-case"),
      Casing::Pascal => Some("PascalCase"),
    };

    let serde_attr = match serde_rename {
      Some(rename) => quote! { #[serde(rename_all = #rename)] },
      None => quote! {},
    };

    tool_impls.push(quote! {
      #[allow(non_camel_case_types)]
      struct #wrapper_ident {
        inner: #struct_ident,
      }

      #[allow(non_camel_case_types)]
      #[derive(::serde::Deserialize)]
      #serde_attr
      struct #args_ident {
        #(#args_fields),*
      }

      impl ::kiruklaw_agent_loop::tools::AgentTool for #wrapper_ident {
        fn descriptor(&self) -> ::kiruklaw_agent_loop::tools::AgentToolDescriptor {
          ::kiruklaw_agent_loop::tools::AgentToolDescriptor::new(
            #tool_name,
            #tool_desc,
            vec![#(#arg_entries),*],
          )
        }

        fn handle(&mut self, args: String) -> ::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output = String> + Send + '_>> {
          ::std::boxed::Box::pin(async move {
            let args: #args_ident = match ::serde_json::from_str(&args) {
              Ok(v) => v,
              Err(e) => return format!("Error: {}", e),
            };
            match self.inner.#method_name(#(#call_args),*).await {
              Ok(result) => result,
              Err(err) => format!("Error: {}", err),
            }
          })
        }
      }
    });

    tool_boxes.push(quote! {
      Box::new(#wrapper_ident { inner: self.clone() })
    });
  }

  let expanded = quote! {
    #impl_block

    #(#tool_impls)*

    impl ::kiruklaw_agent_loop::tools::AgentToolSet for #struct_ident {
      fn tools(&self) -> Vec<Box<dyn ::kiruklaw_agent_loop::tools::AgentTool>> {
        vec![#(#tool_boxes),*]
      }
    }
  };

  expanded.into()
}
