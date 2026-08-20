extern crate proc_macro;

use proc_macro::TokenStream;

mod casing;
mod tool;
mod toolargs;
mod toolset;

/// `#[tool]` is applicable to top-level functions only. It produces
/// a unit struct that implements `AgentTool`. The function name
/// becomes the tool name, and its arguments will be parsed from
/// the JSON using `serde_json`.
///
/// Only JSON schema primitives are supported, i.e. numeric primitives
/// and `String`. The return value must be a `Result<impl Display, impl Display>`
/// as the response will be handed back to the AI agent. It is often
/// convenient to use `anyhow::Error`.
///
/// **Example:**
///
/// ```ignore
/// use kiruklaw_macros::tool;
///
/// #[tool]
/// fn read_file(filepath: String) -> Result<String, anyhow::Error> {
///   Ok(std::fs::read_to_string(filepath).map_err(|e| e.to_string())?)
/// }
/// ```
#[proc_macro_attribute]
pub fn tool(attr: TokenStream, item: TokenStream) -> TokenStream {
  tool::tool(attr, item)
}

/// `#[toolset]` is related to [tool], and most of its documentation
/// is relevant to `#[toolset]` as well.
///
/// Unlike `#[tool]`, `#[toolset]` applies to `impl` blocks, and it
/// allows turning receiver methods into stateful or configurable
/// tools.
///
/// **Example:**
///
/// ```ignore
/// use kiruklaw_macros::toolset;
///
/// #[derive(Debug, Clone)]
/// struct PlanningTools {
///   count: u64,
/// }
///
/// #[toolset]
/// impl PlanningTools {
///   /// Increments the counter.
///   /// @amount Amount to add
///   async fn increment(&mut self, amount: u64) -> Result<String, anyhow::Error> {
///     self.count += amount;
///     Ok(format!("count is now {}", self.count))
///   }
/// }
/// ```
#[proc_macro_attribute]
pub fn toolset(attr: TokenStream, item: TokenStream) -> TokenStream {
  toolset::toolset(attr, item)
}

#[proc_macro_derive(AgentToolArgs, attributes(toolarg, toolargs))]
pub fn derive_agent_tool_args(input: TokenStream) -> TokenStream {
  toolargs::derive_agent_tool_args(input)
}
