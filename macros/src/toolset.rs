// SPDX-License-Identifier: MIT
use crate::casing::Casing;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use std::{collections::HashMap, str::FromStr};
use syn::{
  Attribute, Expr, ExprLit, FnArg, Ident, ImplItem, ItemImpl, Lit, LitStr, Meta, Type,
  parse_macro_input,
};

struct ToolsetAttrs {
  readonly: bool,
  casing: Casing,
  ctx: Option<Type>,
}

impl syn::parse::Parse for ToolsetAttrs {
  fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
    let mut readonly = false;
    let mut casing = None;
    let mut ctx = None;
    while !input.is_empty() {
      let ident: Ident = input.parse()?;
      if ident == "readonly" {
        readonly = true;
      } else if ident == "casing" {
        input.parse::<syn::Token![=]>()?;
        let lit: LitStr = input.parse()?;
        casing = Some(Casing::from_str(&lit.value()).map_err(|e| syn::Error::new(lit.span(), e))?);
      } else if ident == "ctx" {
        input.parse::<syn::Token![=]>()?;
        ctx = Some(input.parse::<Type>()?);
      } else {
        return Err(syn::Error::new(
          ident.span(),
          "expected `readonly`, `casing`, or `ctx`",
        ));
      }
      if input.is_empty() {
        break;
      }
      input.parse::<syn::Token![,]>()?;
    }
    Ok(ToolsetAttrs {
      readonly,
      casing: casing.unwrap_or(Casing::Snake),
      ctx,
    })
  }
}

pub(crate) fn toolset(attr: TokenStream, item: TokenStream) -> TokenStream {
  let impl_block = parse_macro_input!(item as ItemImpl);

  let ToolsetAttrs { casing, readonly, ctx } = match syn::parse(attr) {
    Ok(a) => a,
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
      .into();
    }
  };

  let struct_name = struct_ident.to_string();
  let namespace = casing.recase(&struct_name);

  let mut descriptors = Vec::new();
  let mut args_structs = Vec::new();
  let mut handle_arms = Vec::new();

  for impl_item in &impl_block.items {
    let ImplItem::Fn(method) = impl_item else { continue };

    let self_receiver = method.sig.inputs.first().and_then(|arg| match arg {
      FnArg::Receiver(r) => Some(r),
      _ => None,
    });

    let has_ctx_arg = ctx.as_ref().is_some_and(|ctx_type| {
      let after_self = if self_receiver.is_some() { 1usize } else { 0 };
      let Some(FnArg::Typed(pt)) = method.sig.inputs.iter().nth(after_self) else {
        return false;
      };
      let Type::Reference(r) = &*pt.ty else {
        return false;
      };
      let inner = &*r.elem;
      quote!(#inner).to_string() == quote!(#ctx_type).to_string()
    });

    if readonly {
      if let Some(r) = self_receiver {
        if r.mutability.is_some() {
          return syn::Error::new_spanned(
            r,
            "#[toolset(readonly)] methods must take &self, not &mut self",
          )
          .to_compile_error()
          .into();
        }
      }
    }

    let method_name = &method.sig.ident;
    let method_name_str = method_name.to_string();
    let recased_method = casing.recase(&method_name_str);
    let tool_name = format!("{}::{}", namespace, recased_method);
    let args_ident = Ident::new(
      &format!("{}{}Args", struct_name, Casing::Pascal.recase(&method_name_str)),
      Span::call_site(),
    );

    let raw_docs = extract_doc_lines(&method.attrs);
    let arg_descs = parse_arg_descriptions(&raw_docs);
    let tool_desc = raw_docs
      .iter()
      .filter(|line| !line.starts_with('@'))
      .cloned()
      .collect::<Vec<_>>()
      .join("\n");

    let mut args_fields = Vec::new();
    let mut call_args = Vec::new();

    let mut args_offset = 0usize;
    if self_receiver.is_some() { args_offset += 1; }
    if has_ctx_arg { args_offset += 1; }

    for (arg_i, arg) in method.sig.inputs.iter().enumerate() {
      if arg_i < args_offset { continue }

      if let FnArg::Receiver(r) = arg {
        return syn::Error::new_spanned(r, "unexpected second receiver")
          .to_compile_error()
          .into();
      }
      let FnArg::Typed(pt) = arg else { continue };
      let syn::Pat::Ident(ident) = &*pt.pat else { continue };
      let name = &ident.ident;
      let name_str = name.to_string();
      let ty = &pt.ty;

      let doc = arg_descs.get(&name_str).cloned().unwrap_or_default();
      let field_attr = if doc.is_empty() {
        quote! { #name: #ty }
      } else {
        quote! { #[toolarg(desc = #doc)] #name: #ty }
      };
      args_fields.push(field_attr);
      call_args.push(quote! { args.#name });
    }

    descriptors.push(quote! {
      ::kiruklaw_agent_loop::tools::AgentToolDescriptor::new(
        #tool_name,
        #tool_desc,
        #args_ident::tool_args(),
      )
    });

    let serde_rename_all = casing.to_serde_rename();
    let casing_str = casing.as_str();
    let serde_attr = match serde_rename_all {
      Some(r) => quote! { #[serde(rename_all = #r)] },
      None => quote! {},
    };
    args_structs.push(quote! {
      #[derive(::kiruklaw_agent_loop::macros::AgentToolArgs, ::serde::Deserialize)]
      #[toolargs(casing = #casing_str)]
      #serde_attr
      struct #args_ident {
        #(#args_fields),*
      }
    });

    let call_expr = match (self_receiver.is_some(), has_ctx_arg) {
      (true, true)   => quote! { self.#method_name(_ctx, #(#call_args),*).await },
      (true, false)  => quote! { self.#method_name(#(#call_args),*).await },
      (false, true)  => quote! { Self::#method_name(_ctx, #(#call_args),*).await },
      (false, false) => quote! { Self::#method_name(#(#call_args),*).await },
    };

    handle_arms.push(quote! {
      #recased_method => {
        ::std::boxed::Box::pin(async move {
          let args: #args_ident = match ::serde_json::from_str(&args) {
            Ok(v) => v,
            Err(e) => return format!("Error: {}", e),
          };
          match #call_expr {
            Ok(result) => result,
            Err(err) => format!("Error: {}", err),
          }
        })
      }
    })
  }

  let ctx_path: syn::Type = ctx.unwrap_or_else(|| syn::parse_quote!(()));

  let (set_trait_path, toolset_enum_path, handle_self_tok) = if readonly {
    (
      quote! { ::kiruklaw_agent_loop::tools::AgentToolset },
      quote! { ::kiruklaw_agent_loop::tools::Toolset::Immutable },
      quote! { &'a self },
    )
  } else {
    (
      quote! { ::kiruklaw_agent_loop::tools::AgentToolsetMut },
      quote! { ::kiruklaw_agent_loop::tools::Toolset::Mutable },
      quote! { &'a mut self },
    )
  };

  quote! {
    #impl_block

    impl #struct_ident {
      pub fn to_toolset(self) -> ::kiruklaw_agent_loop::tools::Toolset<#ctx_path> {
        #toolset_enum_path(Box::new(self))
      }
    }

    #(#args_structs)*

    impl #set_trait_path<#ctx_path> for #struct_ident {
      fn name(&self) -> &'static str {
        #namespace
      }

      fn tools(&self) -> Vec<::kiruklaw_agent_loop::tools::AgentToolDescriptor> {
        vec![#(#descriptors),*]
      }

      fn handle<'a>(#handle_self_tok, _ctx: &'a #ctx_path, tool_name: &str, args: String) -> ::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output = String> + Send + 'a>> {
        match tool_name {
          #(#handle_arms)*
          _ => {
            let name = tool_name.to_string();
            ::std::boxed::Box::pin(async move {
              format!("Error: unknown tool {}", name)
            })
          }
        }
      }
    }
  }.into()
}

fn extract_doc_lines(attrs: &[Attribute]) -> Vec<String> {
  let mut lines = Vec::new();
  for attr in attrs {
    if attr.path().is_ident("doc") {
      if let Meta::NameValue(nv) = &attr.meta {
        if let Expr::Lit(ExprLit {
          lit: Lit::Str(s), ..
        }) = &nv.value
        {
          let val = s.value();
          let trimmed = val.strip_prefix(' ').unwrap_or(&val);
          lines.push(trimmed.to_string());
        }
      }
    }
  }
  lines
}

fn parse_arg_descriptions(doc_lines: &[String]) -> HashMap<String, String> {
  let mut descs = HashMap::new();
  for line in doc_lines {
    let Some(rest) = line.strip_prefix('@') else {
      continue;
    };
    if let Some((name, desc)) = rest.split_once(' ') {
      let desc = desc.trim().to_string();
      if !desc.is_empty() {
        descs.insert(name.to_string(), desc);
      }
    }
  }
  descs
}
