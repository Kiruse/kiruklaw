// SPDX-License-Identifier: MIT
use crate::casing::Casing;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use std::{collections::HashMap, str::FromStr};
use syn::{
  Attribute, Expr, ExprLit, FnArg, GenericArgument, Ident, ImplItem, ItemImpl, Lit, LitStr, Meta, Path, PathArguments, Type, parse_macro_input,
};

struct ToolsetAttrs {
  casing: Casing,
  strip_prefix: Option<(String, Span)>,
  strip_suffix: Option<(String, Span)>,
}

impl syn::parse::Parse for ToolsetAttrs {
  fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
    let mut casing = None;
    let mut strip_prefix = None;
    let mut strip_suffix = None;
    while !input.is_empty() {
      let ident: Ident = input.parse()?;
      match ident.to_string().as_str() {
        "casing" => {
          input.parse::<syn::Token![=]>()?;
          let lit: LitStr = input.parse()?;
          casing = Some(
            Casing::from_str(&lit.value()).map_err(|e| syn::Error::new(lit.span(), e))?,
          );
        }
        "strip_prefix" | "strip_suffix" => {
          input.parse::<syn::Token![=]>()?;
          let lit: LitStr = input.parse()?;
          let value = (lit.value(), lit.span());
          if ident == "strip_prefix" {
            strip_prefix = Some(value);
          } else {
            strip_suffix = Some(value);
          }
        }
        other => {
          return Err(syn::Error::new(
            ident.span(),
            format!(
              "expected `casing`, `strip_prefix`, or `strip_suffix`, found `{other}`"
            ),
          ));
        }
      }
      if input.is_empty() {
        break;
      }
      input.parse::<syn::Token![,]>()?;
    }
    Ok(ToolsetAttrs {
      casing: casing.unwrap_or(Casing::Snake),
      strip_prefix,
      strip_suffix,
    })
  }
}

pub(crate) fn toolset(attr: TokenStream, item: TokenStream) -> TokenStream {
  let impl_block = parse_macro_input!(item as ItemImpl);

  let ToolsetAttrs {
    casing,
    strip_prefix,
    strip_suffix,
  } = match syn::parse(attr) {
    Ok(a) => a,
    Err(e) => return e.to_compile_error().into(),
  };

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

  let self_ty = &impl_block.self_ty;
  let struct_name = struct_ident.to_string();
  let namespace = match apply_strips(
    casing.recase(&struct_name),
    &casing,
    strip_prefix,
    strip_suffix,
    &struct_name,
  ) {
    Ok(n) => n,
    Err(e) => return e.to_compile_error().into(),
  };

  let InspectImplResult {
    readonly,
    is_trait_impl,
    ctx_path,
    trait_path,
  } = match inspect_impl(&impl_block) {
    Ok(r) => r,
    Err(e) => {
      return e.to_compile_error().into();
    }
  };

  let mut descriptors = Vec::new();
  let mut args_structs = Vec::new();
  let mut handle_arms = Vec::new();

  for impl_item in &impl_block.items {
    let ImplItem::Fn(method) = impl_item else { continue };

    let self_receiver = method.sig.inputs.first().and_then(|arg| match arg {
      FnArg::Receiver(r) => Some(r),
      _ => None,
    });

    let has_ctx_arg = {
      let after_self = if self_receiver.is_some() { 1usize } else { 0 };
      method.sig.inputs
        .iter()
        .nth(after_self)
        .is_some_and(|arg| {
          let FnArg::Typed(pt) = arg else { return false };
          let ty = *pt.ty.clone();
          quote!(#ty).to_string() == quote!(#ctx_path).to_string()
        })
    };

    if readonly {
      if let Some(r) = self_receiver {
        if r.mutability.is_some() {
          return syn::Error::new_spanned(
            r,
            "AgentToolset methods must take &self, not &mut self",
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
    if self_receiver.is_some() { args_offset += 1 };
    if has_ctx_arg { args_offset += 1 };

    for (arg_i, arg) in method.sig.inputs.iter().enumerate() {
      if arg_i < args_offset { continue };

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
      let field_attr = match doc.is_empty() {
        true  => quote! { #name: #ty },
        false => quote! { #[toolarg(desc = #doc)] #name: #ty },
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

    let casing_str = casing.as_str();
    let serde_attr = casing.to_serde_rename()
      .map(|rename| quote! { #[serde(rename_all = #rename)] })
      .unwrap_or_default();
    args_structs.push(quote! {
      #[derive(::kiruklaw_agent_loop::macros::AgentToolArgs, ::serde::Deserialize)]
      #[toolargs(casing = #casing_str)]
      #serde_attr
      struct #args_ident {
        #(#args_fields),*
      }
    });

    let call_expr = match (self_receiver.is_some(), has_ctx_arg) {
      (true, true) => quote! { self.#method_name(_ctx, #(#call_args),*).await },
      (true, false) => quote! { self.#method_name(#(#call_args),*).await },
      (false, true) => quote! { Self::#method_name(_ctx, #(#call_args),*).await },
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
    });
  }

  let handle_self_tok = match readonly {
    true  => quote! { &'__kh self },
    false => quote! { &'__kh mut self },
  };

  let trait_methods = quote! {
    fn name(&self) -> &'static str {
      #namespace
    }

    fn tools(&self) -> Vec<::kiruklaw_agent_loop::tools::AgentToolDescriptor> {
      vec![#(#descriptors),*]
    }

    fn handle<'__kh>(
      #handle_self_tok,
      _ctx: #ctx_path,
      tool_name: &str,
      args: String,
    ) -> ::std::pin::Pin<
      ::std::boxed::Box<dyn ::std::future::Future<Output = String> + Send + '__kh>,
    >
    where #ctx_path: '__kh
    {
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
  };

  if is_trait_impl {
    let trait_path = trait_path.unwrap();
    let (impl_generics, _, where_clause) = impl_block.generics.split_for_impl();
    let impl_attrs = &impl_block.attrs;
    let impl_items = &impl_block.items;

    let toolset_enum_variant = match readonly {
      true  => quote! { ::kiruklaw_agent_loop::tools::Toolset::Immutable },
      false => quote! { ::kiruklaw_agent_loop::tools::Toolset::Mutable },
    };

    quote! {
      #(#impl_attrs)*
      impl #impl_generics #self_ty #where_clause {
        #(#impl_items)*

        pub fn to_toolset(self) -> ::kiruklaw_agent_loop::tools::Toolset<#ctx_path> {
          #toolset_enum_variant(::std::boxed::Box::new(self))
        }
      }

      #(#impl_attrs)*
      impl #impl_generics #trait_path for #self_ty #where_clause {
        #trait_methods
      }

      #(#args_structs)*
    }.into()
  } else {
    quote! {
      #impl_block

      impl #self_ty {
        pub fn to_toolset(self) -> ::kiruklaw_agent_loop::tools::Toolset<()> {
          ::kiruklaw_agent_loop::tools::Toolset::Immutable(::std::boxed::Box::new(self))
        }
      }

      #(#args_structs)*

      impl ::kiruklaw_agent_loop::tools::AgentToolset<()> for #self_ty {
        #trait_methods
      }
    }.into()
  }
}

fn apply_strips(
  mut namespace: String,
  casing: &Casing,
  strip_prefix: Option<(String, Span)>,
  strip_suffix: Option<(String, Span)>,
  struct_name: &str,
) -> Result<String, syn::Error> {
  let sep = casing.separator();
  if let Some((prefix, span)) = strip_prefix {
    let pattern = format!("{}{}", casing.recase(&prefix), sep);
    namespace = match namespace.strip_prefix(&pattern) {
      Some(rest) if !rest.is_empty() => rest.to_string(),
      Some(_) => {
        return Err(syn::Error::new(
          span,
          format!(
            "stripping prefix `{pattern}` from `{struct_name}` would produce an empty toolset name"
          ),
        ));
      }
      None => {
        return Err(syn::Error::new(
          span,
          format!("toolset name `{namespace}` does not start with `{pattern}`"),
        ));
      }
    };
  }
  if let Some((suffix, span)) = strip_suffix {
    let pattern = format!("{}{}", sep, casing.recase(&suffix));
    namespace = match namespace.strip_suffix(&pattern) {
      Some(rest) if !rest.is_empty() => rest.to_string(),
      Some(_) => {
        return Err(syn::Error::new(
          span,
          format!(
            "stripping suffix `{pattern}` from `{struct_name}` would produce an empty toolset name"
          ),
        ));
      }
      None => {
        return Err(syn::Error::new(
          span,
          format!("toolset name `{namespace}` does not end with `{pattern}`"),
        ));
      }
    };
  }
  Ok(namespace)
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

struct InspectImplResult {
  pub readonly: bool,
  pub is_trait_impl: bool,
  pub ctx_path: Type,
  pub trait_path: Option<Path>,
}

fn inspect_impl(impl_block: &ItemImpl) -> Result<InspectImplResult, syn::Error> {
  match &impl_block.trait_ {
    Some((_, tp, _)) => {
      let last = tp.segments.last().expect("empty trait path");
      let name = last.ident.to_string();
      let readonly = match name.as_str() {
        "AgentToolset" => true,
        "AgentToolsetMut" => false,
        other => {
          return Err(syn::Error::new_spanned(
            tp,
            format!("expected `AgentToolset` or `AgentToolsetMut`, found `{other}`"),
          ));
        }
      };
      let ctx_path = match &last.arguments {
        PathArguments::AngleBracketed(ab) => {
          if ab.args.len() != 1 {
            return Err(syn::Error::new_spanned(
              &last.arguments,
              "expected at most one generic argument for the context type",
            ));
          }
          match &ab.args[0] {
            GenericArgument::Type(t) => t.clone(),
            _ => {
              return Err(syn::Error::new_spanned(
                &last.arguments,
                "expected a type argument for the context type",
              ));
            }
          }
        }
        PathArguments::None => syn::parse_quote!(()),
        _ => {
          return Err(syn::Error::new_spanned(
            &last.arguments,
            "expected angle-bracketed or no generic arguments on the trait",
          ));
        }
      };
      Ok(InspectImplResult {
        readonly,
        is_trait_impl: true,
        ctx_path,
        trait_path: Some(tp.clone()),
      })
    }
    None => Ok(InspectImplResult {
      readonly: true,
      is_trait_impl: false,
      ctx_path: syn::parse_quote!(()),
      trait_path: None,
    }),
  }
}
