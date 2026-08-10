use kiruklaw_agent_loop::tools::AgentTool;
use kiruklaw_macros::tool;

#[tool]
/// Adds two numbers together.
/// @a First operand
/// @b Second operand
fn add(
  a: i64,
  b: i64,
) -> Result<String, anyhow::Error> {
  Ok(format!("{a} + {b} = {}", a + b))
}

#[test]
fn descriptor_fields() {
  let t = Add;
  let desc = t.descriptor();
  assert_eq!(desc.name, "add");
  assert_eq!(desc.description, "Adds two numbers together.");
  assert_eq!(desc.args.len(), 2);

  assert_eq!(desc.args[0].name, "a");
  assert_eq!(desc.args[0].description.as_deref(), Some("First operand"));
  assert!(desc.args[0].required);

  assert_eq!(desc.args[1].name, "b");
  assert_eq!(desc.args[1].description.as_deref(), Some("Second operand"));
  assert!(desc.args[1].required);
}

#[test]
fn handle_success() {
  let result = Add.handle(r#"{"a": 1, "b": 2}"#.to_string());
  assert_eq!(result, "1 + 2 = 3");
}

#[test]
fn handle_parse_error() {
  let t = Add;
  let result = t.handle("invalid json".to_string());
  assert!(result.starts_with("Error: "));
}

#[tool]
fn always_err() -> Result<String, anyhow::Error> {
  Err(anyhow::anyhow!("something went wrong"))
}

#[test]
fn handle_fn_error() {
  let t = AlwaysErr;
  let result = t.handle(r#"{}"#.to_string());
  assert_eq!(result, "Error: something went wrong");
}

#[tool]
/// Greets a person.
/// @name Name of the person
/// @excited Add excitement
fn greet(
  name: String,
  excited: Option<bool>,
) -> Result<String, anyhow::Error> {
  match excited {
    Some(true) => Ok(format!("Hello, {}!!!", name)),
    _ => Ok(format!("Hello, {}.", name)),
  }
}

#[test]
fn optional_arg_descriptor() {
  let t = Greet;
  let desc = t.descriptor();
  assert_eq!(desc.args.len(), 2);

  assert_eq!(desc.args[0].name, "name");
  assert!(desc.args[0].required);

  assert_eq!(desc.args[1].name, "excited");
  assert!(!desc.args[1].required);
}

#[test]
fn optional_arg_handle() {
  let t = Greet;
  let result = t.handle(r#"{"name": "world"}"#.to_string());
  assert_eq!(result, "Hello, world.");

  let result = t.handle(r#"{"name": "world", "excited": true}"#.to_string());
  assert_eq!(result, "Hello, world!!!");
}

#[tool]
fn no_args() -> Result<String, anyhow::Error> {
  Ok("pong".to_string())
}

#[test]
fn no_args_descriptor() {
  let t = NoArgs;
  let desc = t.descriptor();
  assert_eq!(desc.name, "no_args");
  assert!(desc.args.is_empty());
}

#[test]
fn no_args_handle() {
  let t = NoArgs;
  let result = t.handle(r#"{}"#.to_string());
  assert_eq!(result, "pong");
}

#[tool]
fn no_docs(x: i64) -> Result<String, anyhow::Error> {
  Ok(format!("{}", x))
}

#[test]
fn missing_docs() {
  let t = NoDocs;
  let desc = t.descriptor();
  assert_eq!(desc.name, "no_docs");
  assert!(desc.description.is_empty());
  assert_eq!(desc.args.len(), 1);
  assert!(desc.args[0].description.is_none());
}

#[tool(casing = "camel")]
/// Adds two numbers together.
/// @first_operand First operand
/// @second_operand Second operand
fn add_camel(
  first_operand: i64,
  second_operand: i64,
) -> Result<String, anyhow::Error> {
  Ok(format!("{}", first_operand + second_operand))
}

#[test]
fn casing_camel() {
  let t = AddCamel;
  let desc = t.descriptor();
  assert_eq!(desc.name, "addCamel");
  assert_eq!(desc.args[0].name, "firstOperand");
  assert_eq!(desc.args[1].name, "secondOperand");
  assert_eq!(
    t.handle(r#"{"firstOperand": 1, "secondOperand": 2}"#.to_string()),
    "3"
  );
}

#[tool(casing = "kebab")]
/// Adds two numbers together.
/// @first_operand First operand
/// @second_operand Second operand
fn add_kebab(
  first_operand: i64,
  second_operand: i64,
) -> Result<String, anyhow::Error> {
  Ok(format!("{}", first_operand + second_operand))
}

#[test]
fn casing_kebab() {
  let t = AddKebab;
  let desc = t.descriptor();
  assert_eq!(desc.name, "add-kebab");
  assert_eq!(desc.args[0].name, "first-operand");
  assert_eq!(desc.args[1].name, "second-operand");
  assert_eq!(
    t.handle(r#"{"first-operand": 1, "second-operand": 2}"#.to_string()),
    "3"
  );
}

#[tool(casing = "pascal")]
/// Adds two numbers together.
/// @first_operand First operand
/// @second_operand Second operand
fn add_pascal(
  first_operand: i64,
  second_operand: i64,
) -> Result<String, anyhow::Error> {
  Ok(format!("{}", first_operand + second_operand))
}

#[test]
fn casing_pascal() {
  let t = AddPascal;
  let desc = t.descriptor();
  assert_eq!(desc.name, "AddPascal");
  assert_eq!(desc.args[0].name, "FirstOperand");
  assert_eq!(desc.args[1].name, "SecondOperand");
  assert_eq!(
    t.handle(r#"{"FirstOperand": 1, "SecondOperand": 2}"#.to_string()),
    "3"
  );
}

#[test]
fn casing_default_is_snake() {
  let t = Add;
  let desc = t.descriptor();
  assert_eq!(desc.name, "add");
}
