extern crate proc_macro;

use proc_macro::TokenStream;

mod casing;
mod tool;

#[proc_macro_attribute]
pub fn tool(attr: TokenStream, item: TokenStream) -> TokenStream {
  tool::tool(attr, item)
}
