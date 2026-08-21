// SPDX-License-Identifier: MIT
use kiruklaw_agent_loop::tools::AgentToolset;
use kiruklaw_macros::tool;

#[tool]
#[allow(unused)]
/// Adds two numbers together.
/// @a First operand
/// @b Second operand
async fn add(a: i64, b: i64) -> Result<String, anyhow::Error> {
  Ok(format!("{a} + {b} = {}", a + b))
}

#[tokio::test]
async fn descriptor_fields() {
  let t = Add;
  assert_eq!(t.name(), "add");
  let descs = t.tools();
  assert_eq!(descs.len(), 1);
  let desc = &descs[0];
  assert_eq!(desc.name, "add::add");
  assert_eq!(desc.description, "Adds two numbers together.");
  assert_eq!(desc.args.len(), 2);

  assert_eq!(desc.args[0].name, "a");
  assert_eq!(desc.args[0].description.as_deref(), Some("First operand"));
  assert!(desc.args[0].required);

  assert_eq!(desc.args[1].name, "b");
  assert_eq!(desc.args[1].description.as_deref(), Some("Second operand"));
  assert!(desc.args[1].required);
}

#[tokio::test]
async fn handle_success() {
  let t = Add;
  let result = t.handle(&(), "add", r#"{"a": 1, "b": 2}"#.to_string()).await;
  assert_eq!(result, "1 + 2 = 3");
}

#[tokio::test]
async fn handle_parse_error() {
  let t = Add;
  let result = t.handle(&(), "add", "invalid json".to_string()).await;
  assert!(result.starts_with("Error: "));
}

#[tool]
#[allow(unused)]
async fn always_err() -> Result<String, anyhow::Error> {
  Err(anyhow::anyhow!("something went wrong"))
}

#[tokio::test]
async fn handle_fn_error() {
  let t = AlwaysErr;
  let result = t.handle(&(), "always_err", r#"{}"#.to_string()).await;
  assert_eq!(result, "Error: something went wrong");
}

#[tool]
#[allow(unused)]
/// Greets a person.
/// @name Name of the person
/// @excited Add excitement
async fn greet(name: String, excited: Option<bool>) -> Result<String, anyhow::Error> {
  match excited {
    Some(true) => Ok(format!("Hello, {}!!!", name)),
    _ => Ok(format!("Hello, {}.", name)),
  }
}

#[tokio::test]
async fn optional_arg_descriptor() {
  let descs = Greet.tools();
  assert_eq!(descs.len(), 1);
  let desc = &descs[0];
  assert_eq!(desc.args.len(), 2);

  assert_eq!(desc.args[0].name, "name");
  assert!(desc.args[0].required);

  assert_eq!(desc.args[1].name, "excited");
  assert!(!desc.args[1].required);
}

#[tokio::test]
async fn optional_arg_handle() {
  let t = Greet;
  let result = t.handle(&(), "greet", r#"{"name": "world"}"#.to_string()).await;
  assert_eq!(result, "Hello, world.");

  let result = t.handle(&(), "greet", r#"{"name": "world", "excited": true}"#.to_string()).await;
  assert_eq!(result, "Hello, world!!!");
}

#[tool]
#[allow(unused)]
async fn no_args() -> Result<String, anyhow::Error> {
  Ok("pong".to_string())
}

#[tokio::test]
async fn no_args_descriptor() {
  let t = NoArgs;
  let descs = t.tools();
  assert_eq!(descs.len(), 1);
  assert_eq!(descs[0].name, "no_args::no_args");
  assert!(descs[0].args.is_empty());
}

#[tokio::test]
async fn no_args_handle() {
  let t = NoArgs;
  let result = t.handle(&(), "no_args", r#"{}"#.to_string()).await;
  assert_eq!(result, "pong");
}

#[tool]
#[allow(unused)]
async fn no_docs(x: i64) -> Result<String, anyhow::Error> {
  Ok(format!("{}", x))
}

#[tokio::test]
async fn missing_docs() {
  let t = NoDocs;
  let descs = t.tools();
  assert_eq!(descs[0].name, "no_docs::no_docs");
  assert!(descs[0].description.is_empty());
  assert_eq!(descs[0].args.len(), 1);
  assert!(descs[0].args[0].description.is_none());
}

#[tool(casing = "camel")]
#[allow(unused)]
/// Adds two numbers together.
/// @first_operand First operand
/// @second_operand Second operand
async fn add_camel(first_operand: i64, second_operand: i64) -> Result<String, anyhow::Error> {
  Ok(format!("{}", first_operand + second_operand))
}

#[tokio::test]
async fn casing_camel() {
  let t = AddCamel;
  let descs = t.tools();
  assert_eq!(descs[0].name, "addCamel::addCamel");
  assert_eq!(descs[0].args[0].name, "firstOperand");
  assert_eq!(descs[0].args[1].name, "secondOperand");
  assert_eq!(
    t.handle(&(), "addCamel", r#"{"firstOperand": 1, "secondOperand": 2}"#.to_string()).await,
    "3"
  );
}

#[tool(casing = "kebab")]
#[allow(unused)]
/// Adds two numbers together.
/// @first_operand First operand
/// @second_operand Second operand
async fn add_kebab(first_operand: i64, second_operand: i64) -> Result<String, anyhow::Error> {
  Ok(format!("{}", first_operand + second_operand))
}

#[tokio::test]
async fn casing_kebab() {
  let t = AddKebab;
  let descs = t.tools();
  assert_eq!(descs[0].name, "add-kebab::add-kebab");
  assert_eq!(descs[0].args[0].name, "first-operand");
  assert_eq!(descs[0].args[1].name, "second-operand");
  assert_eq!(
    t.handle(&(), "add-kebab", r#"{"first-operand": 1, "second-operand": 2}"#.to_string()).await,
    "3"
  );
}

#[tool(casing = "pascal")]
#[allow(unused)]
/// Adds two numbers together.
/// @first_operand First operand
/// @second_operand Second operand
async fn add_pascal(first_operand: i64, second_operand: i64) -> Result<String, anyhow::Error> {
  Ok(format!("{}", first_operand + second_operand))
}

#[tokio::test]
async fn casing_pascal() {
  let t = AddPascal;
  let descs = t.tools();
  assert_eq!(descs[0].name, "AddPascal::AddPascal");
  assert_eq!(descs[0].args[0].name, "FirstOperand");
  assert_eq!(descs[0].args[1].name, "SecondOperand");
  assert_eq!(
    t.handle(
      &(),
      "AddPascal",
      r#"{"FirstOperand": 1, "SecondOperand": 2}"#.to_string()
    )
    .await,
    "3"
  );
}

#[tokio::test]
async fn casing_default_is_snake() {
  let t = Add;
  let descs = t.tools();
  assert_eq!(descs[0].name, "add::add");
}
