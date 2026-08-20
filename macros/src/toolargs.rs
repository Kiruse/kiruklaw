// SPDX-License-Identifier: MIT
use crate::casing::Casing;
use proc_macro::TokenStream;
use quote::quote;
use std::str::FromStr;
use syn::{
  Data, DeriveInput, Field, GenericArgument, PathArguments, Token, Type, parse_macro_input,
};

struct ToolArgAttr {
  rename: Option<String>,
  desc: Option<String>,
}

enum PrimitiveKind {
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
}

impl FromStr for PrimitiveKind {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "String" => Ok(Self::String),
      "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
      | "usize" => Ok(PrimitiveKind::Int),
      "f32" | "f64" => Ok(PrimitiveKind::Number),
      "bool" => Ok(PrimitiveKind::Bool),
      _ => Err(format!("Unsupported type name {s}")),
    }
  }
}

impl quote::ToTokens for PrimitiveKind {
  fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
    tokens.extend(match self {
      PrimitiveKind::String => quote! {
        ::kiruklaw_agent_loop::tools::AgentToolArgPrimitive::STRING
      },
      PrimitiveKind::Int => quote! {
        ::kiruklaw_agent_loop::tools::AgentToolArgPrimitive::INT
      },
      PrimitiveKind::Number => quote! {
        ::kiruklaw_agent_loop::tools::AgentToolArgPrimitive::NUMBER
      },
      PrimitiveKind::Bool => quote! {
        ::kiruklaw_agent_loop::tools::AgentToolArgPrimitive::BOOL
      },
    });
  }
}

fn parse_tool_arg_attr(field: &Field) -> ToolArgAttr {
  let mut rename = None;
  let mut desc = None;
  for attr in &field.attrs {
    if !attr.path().is_ident("toolarg") {
      continue;
    }
    let syn::Meta::List(list) = &attr.meta else {
      continue;
    };
    let parser = list.parse_args_with(|input: syn::parse::ParseStream| {
      let mut kvs = Vec::new();
      loop {
        let ident: syn::Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let lit: syn::LitStr = input.parse()?;
        kvs.push((ident.to_string(), lit.value()));
        if input.is_empty() {
          break;
        }
        input.parse::<Token![,]>()?;
      }
      Ok(kvs)
    });
    if let Ok(pairs) = parser {
      for (key, value) in pairs {
        if key == "rename" {
          rename = Some(value);
        } else if key == "desc" {
          desc = Some(value);
        }
      }
    }
  }
  ToolArgAttr { rename, desc }
}

fn parse_container_attr(attrs: &[syn::Attribute]) -> Result<Casing, syn::Error> {
  for attr in attrs {
    if !attr.path().is_ident("toolargs") {
      continue;
    }
    let syn::Meta::List(list) = &attr.meta else {
      return Err(syn::Error::new_spanned(
        attr,
        "expected #[toolargs(casing = \"...\")]",
      ));
    };
    let casing: Casing = list.parse_args_with(|input: syn::parse::ParseStream| {
      let ident: syn::Ident = input.parse()?;
      if ident != "casing" {
        return Err(syn::Error::new(ident.span(), "expected `casing`"));
      }
      input.parse::<Token![=]>()?;
      let lit: syn::LitStr = input.parse()?;
      Casing::from_str(&lit.value()).map_err(|e| syn::Error::new(lit.span(), e))
    })?;
    return Ok(casing);
  }
  Ok(Casing::Snake)
}

fn get_arg_type(ty: &Type) -> Result<(proc_macro2::TokenStream, bool), String> {
  let Type::Path(tp) = ty else {
    return Err(format!("Invalid argument type {ty:?}"));
  };

  let seg = tp
    .path
    .segments
    .last()
    .ok_or("Unexpected empty path type".to_string())?;

  if seg.ident.to_string() == "Option" {
    if let PathArguments::AngleBracketed(ab) = &seg.arguments {
      if let Some(GenericArgument::Type(inner)) = ab.args.first() {
        let kind = PrimitiveKind::parse(inner)?;
        return Ok((quote! { #kind.nullable().into() }, false));
      }
    }
    return Err(format!("Invalid argument type {ty:?}"));
  }

  let kind = PrimitiveKind::parse(ty)?;
  Ok((quote! { #kind.into() }, true))
}

pub(crate) fn derive_agent_tool_args(input: TokenStream) -> TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  let ident = &input.ident;

  let casing = match parse_container_attr(&input.attrs) {
    Ok(c) => c,
    Err(e) => return e.to_compile_error().into(),
  };

  let Data::Struct(data) = &input.data else {
    return syn::Error::new_spanned(&input, "AgentToolArgs can only be derived on structs")
      .to_compile_error()
      .into();
  };

  let mut entries = Vec::new();

  for field in &data.fields {
    let Some(field_ident) = field.ident.as_ref() else {
      continue;
    };
    let field_name = field_ident.to_string();
    let ty = &field.ty;
    let attr = parse_tool_arg_attr(field);
    let arg_name = attr.rename.unwrap_or_else(|| casing.recase(&field_name));

    let (type_tokens, required) = match get_arg_type(ty) {
      Ok(v) => v,
      Err(e) => {
        return syn::Error::new_spanned(ty, format!("unsupported type for AgentToolArgs: {e}"))
          .to_compile_error()
          .into();
      }
    };

    let mut builder = quote! {
      ::kiruklaw_agent_loop::tools::AgentToolArg::new(#arg_name, #type_tokens)
    };
    if let Some(d) = attr.desc {
      builder = quote! { #builder.with_description(#d.to_string()) };
    }
    if required {
      builder = quote! { #builder.required() };
    }

    entries.push(builder);
  }

  quote! {
    impl #ident {
      fn tool_args() -> Vec<::kiruklaw_agent_loop::tools::AgentToolArg> {
        vec![#(#entries),*]
      }
    }
  }.into()
}
