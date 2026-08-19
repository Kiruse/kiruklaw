use crate::casing::Casing;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use std::{collections::HashMap, str::FromStr};
use syn::{
  parse_macro_input, Attribute, Expr, ExprLit, FnArg, GenericArgument, Ident, ItemFn, Lit,
  Meta, Pat, PathArguments, Type,
};

pub(crate) fn tool(attr: TokenStream, item: TokenStream) -> TokenStream {
  let func = parse_macro_input!(item as ItemFn);
  let casing = match parse_casing_attr(&attr) {
    Ok(c) => c,
    Err(e) => return e.to_compile_error().into(),
  };

  let fn_name = &func.sig.ident;
  let fn_name_str = fn_name.to_string();
  let tool_name = casing.recase(&fn_name_str);
  let struct_name = Casing::Pascal.recase(&fn_name_str);
  let struct_ident = Ident::new(&struct_name, Span::call_site());
  let args_ident = Ident::new(&format!("{}Args", struct_name), Span::call_site());

  let raw_docs = extract_doc_lines(&func.attrs);
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

  for arg in &func.sig.inputs {
    match arg {
      FnArg::Receiver(r) => {
        return syn::Error::new_spanned(r, "#[tool] can only be applied to top-level functions")
          .to_compile_error()
          .into();
      }
      FnArg::Typed(pt) => {
        if let Pat::Ident(ident) = &*pt.pat {
          let name = &ident.ident;
          let name_str = name.to_string();
          let llm_name = casing.recase(&name_str);
          let ty = &pt.ty;

          let (type_tokens, required) = match get_arg_type(ty) {
            Ok(v) => v,
            Err(e) => {
              return syn::Error::new_spanned(
                ty,
                format!("unsupported argument type for #[tool]: {e}"),
              )
              .to_compile_error()
              .into();
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
      }
    }
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

  let expanded = quote! {
    #func

    #[allow(non_camel_case_types)]
    pub struct #struct_ident;

    #[allow(non_camel_case_types)]
    #[derive(::serde::Deserialize)]
    #serde_attr
    struct #args_ident {
      #(#args_fields),*
    }

    impl ::kiruklaw_agent_loop::tools::AgentToolMut for #struct_ident {
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
          match #fn_name(#(#call_args),*) {
            Ok(result) => result,
            Err(err) => format!("Error: {}", err),
          }
        })
      }
    }
  };

  expanded.into()
}

pub(crate) fn extract_doc_lines(attrs: &[Attribute]) -> Vec<String> {
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

pub(crate) fn parse_arg_descriptions(doc_lines: &[String]) -> HashMap<String, String> {
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

pub(crate) fn parse_casing_attr(attr: &TokenStream) -> Result<Casing, syn::Error> {
  if attr.is_empty() {
    return Ok(Casing::Snake);
  }
  syn::parse(attr.clone())
}

pub(crate) fn get_arg_type(ty: &Type) -> Result<(proc_macro2::TokenStream, bool), String> {
  let Type::Path(tp) = ty else {
    return Err(format!("Invalid argument type {ty:?}"));
  };

  let seg = tp.path.segments.last()
    .ok_or("Unexpected empty path type".to_string())?;

  if seg.ident.to_string() == "Option" {
    if let PathArguments::AngleBracketed(ab) = &seg.arguments {
      if let Some(GenericArgument::Type(inner)) = ab.args.first() {
        let kind = PrimitiveKind::parse(inner)?;
        let primitive = kind.tokens();
        return Ok((
          quote! { ::kiruklaw_agent_loop::tools::AgentToolArgType::Primitive(#primitive.nullable()) },
          false,
        ));
      }
    }
    return Err(format!("Invalid argument type {ty:?}"));
  }

  let kind = PrimitiveKind::parse(ty)?;
  let primitive = kind.tokens();
  Ok((
    quote! { ::kiruklaw_agent_loop::tools::AgentToolArgType::Primitive(#primitive) },
    true,
  ))
}

pub(crate) enum PrimitiveKind {
  String,
  Int,
  Number,
  Bool,
}

impl PrimitiveKind {
  fn parse(ty: &Type) -> Result<PrimitiveKind, String> {
    let Type::Path(tp) = ty else {
      return Err(format!("Unsupported rust type {ty:?}"));
    };
    tp.path
      .segments
      .last()
      .ok_or("Empty rust path type".to_string())?
      .ident
      .to_string()
      .parse()
  }

  fn tokens(&self) -> proc_macro2::TokenStream {
    match self {
      PrimitiveKind::String => {
        quote! { ::kiruklaw_agent_loop::tools::AgentToolArgPrimitive::STRING }
      }
      PrimitiveKind::Int => {
        quote! { ::kiruklaw_agent_loop::tools::AgentToolArgPrimitive::INT }
      }
      PrimitiveKind::Number => {
        quote! { ::kiruklaw_agent_loop::tools::AgentToolArgPrimitive::NUMBER }
      }
      PrimitiveKind::Bool => {
        quote! { ::kiruklaw_agent_loop::tools::AgentToolArgPrimitive::BOOL }
      }
    }
  }
}

impl FromStr for PrimitiveKind {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "String" => Ok(Self::String),
      "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
      | "u128" | "usize" => Ok(PrimitiveKind::Int),
      "f32" | "f64" => Ok(PrimitiveKind::Number),
      "bool" => Ok(PrimitiveKind::Bool),
      _ => Err(format!("Unsupported type name {s}")),
    }
  }
}
