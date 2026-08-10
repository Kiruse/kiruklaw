use std::str::FromStr;

use syn::{Ident, LitStr, Token};

pub(crate) enum Casing {
  Camel,
  Kebab,
  Pascal,
  Snake,
}

impl Casing {
  pub fn recase(&self, src: &str) -> String {
    let words: Vec<&str> = src.split('_').filter(|p| !p.is_empty()).collect();
    match self {
      Casing::Camel => capitalize_first(&words, false),
      Casing::Kebab => words.join("-"),
      Casing::Pascal => capitalize_first(&words, true),
      Casing::Snake => words.join("_"),
    }
  }
}

fn capitalize_first(words: &[&str], all: bool) -> String {
  words
    .iter()
    .enumerate()
    .map(|(i, part)| {
      let mut chars = part.chars();
      match chars.next() {
        None => String::new(),
        Some(first) => {
          let upper: String = first.to_uppercase().collect();
          let rest: String = chars.collect();
          if all || i > 0 {
            upper + &rest
          } else {
            first.to_lowercase().collect::<String>() + &rest
          }
        }
      }
    })
    .collect()
}

impl syn::parse::Parse for Casing {
  fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
    let ident: Ident = input.parse()?;
    if ident != "casing" {
      return Err(syn::Error::new(ident.span(), "expected `casing`"));
    }
    input.parse::<Token![=]>()?;
    let lit: LitStr = input.parse()?;
    Casing::from_str(&lit.value()).map_err(|e| syn::Error::new(lit.span(), e))
  }
}

impl FromStr for Casing {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "camel"  => Ok(Casing::Camel),
      "kebab"  => Ok(Casing::Kebab),
      "pascal" => Ok(Casing::Pascal),
      "snake"  => Ok(Casing::Snake),
      _ => Err(format!("Unknown casing variant {s}")),
    }
  }
}
