// SPDX-License-Identifier: MIT
use kiruklaw_macros::AgentToolArgs;

#[derive(AgentToolArgs)]
#[allow(unused)]
struct AddArgs {
  a: i64,
  b: i64,
}

#[test]
fn required_fields() {
  let vec = AddArgs::tool_args();
  assert_eq!(vec.len(), 2);
  assert_eq!(vec[0].name, "a");
  assert!(vec[0].required);
  assert_eq!(vec[1].name, "b");
  assert!(vec[1].required);
}

#[derive(AgentToolArgs)]
#[allow(unused)]
struct GreetArgs {
  name: String,
  excited: Option<bool>,
}

#[test]
fn optional_field() {
  let vec = GreetArgs::tool_args();
  assert_eq!(vec.len(), 2);
  assert!(vec[0].required);
  assert!(!vec[1].required);
}

#[derive(AgentToolArgs)]
#[allow(unused)]
struct TypedArgs {
  s: String,
  i: i64,
  f: f64,
  b: bool,
  opt: Option<String>,
}

#[test]
fn type_mapping() {
  let vec = TypedArgs::tool_args();
  assert_eq!(vec.len(), 5);
  assert_eq!(vec[0].name, "s");
  assert!(vec[0].required);
  assert_eq!(vec[1].name, "i");
  assert!(vec[1].required);
  assert_eq!(vec[2].name, "f");
  assert!(vec[2].required);
  assert_eq!(vec[3].name, "b");
  assert!(vec[3].required);
  assert_eq!(vec[4].name, "opt");
  assert!(!vec[4].required);
}

#[derive(AgentToolArgs)]
#[allow(unused)]
struct AttrArgs {
  #[toolarg(desc = "The name")]
  name: String,
  #[toolarg(rename = "fooBar", desc = "A flag")]
  flag: Option<bool>,
}

#[test]
fn tool_arg_attributes() {
  let vec = AttrArgs::tool_args();
  assert_eq!(vec.len(), 2);
  assert_eq!(vec[0].name, "name");
  assert_eq!(vec[0].description.as_deref(), Some("The name"));
  assert!(vec[0].required);
  assert_eq!(vec[1].name, "fooBar");
  assert_eq!(vec[1].description.as_deref(), Some("A flag"));
  assert!(!vec[1].required);
}

#[derive(AgentToolArgs)]
struct EmptyArgs {}

#[test]
fn no_fields() {
  assert!(EmptyArgs::tool_args().is_empty());
}

#[derive(AgentToolArgs)]
#[toolargs(casing = "camel")]
#[allow(unused)]
struct CamelArgs {
  first_name: String,
  last_name: Option<String>,
}

#[test]
fn casing_camel() {
  let vec = CamelArgs::tool_args();
  assert_eq!(vec[0].name, "firstName");
  assert_eq!(vec[1].name, "lastName");
}

#[derive(AgentToolArgs)]
#[toolargs(casing = "camel")]
#[allow(unused)]
struct RenameOverridesCasing {
  #[toolarg(rename = "customName")]
  first_name: String,
  second_name: String,
}

#[test]
fn rename_overrides_casing() {
  let vec = RenameOverridesCasing::tool_args();
  assert_eq!(vec[0].name, "customName");
  assert_eq!(vec[1].name, "secondName");
}

#[derive(AgentToolArgs)]
#[allow(unused)]
struct DefaultSnake {
  my_field: String,
}

#[test]
fn casing_defaults_to_snake() {
  let vec = DefaultSnake::tool_args();
  assert_eq!(vec[0].name, "my_field");
}
