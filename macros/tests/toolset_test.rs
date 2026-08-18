// SPDX-License-Identifier: MIT
use kiruklaw_agent_loop::tools::AgentToolSet;
use kiruklaw_macros::toolset;

#[derive(Debug, Clone, Default)]
struct MathTools {
  owner: String,
  precision: u8,
}

#[toolset]
impl MathTools {
  /// Adds two numbers together.
  /// @a First operand
  /// @b Second operand
  async fn add(
    &self,
    a: i64,
    b: i64,
  ) -> Result<String, anyhow::Error> {
    if self.precision == 0 {
      return Ok(format!("{} + {} = {}", a, b, a + b));
    }
    Ok(format!("{:.prec$} + {:.prec$} = {:.prec$}", a as f64, b as f64, (a + b) as f64, prec = self.precision as usize))
  }

  /// Greets a person.
  /// @excited Add excitement
  async fn greet(
    &self,
    excited: Option<bool>,
  ) -> Result<String, anyhow::Error> {
    match excited {
      Some(true) => Ok(format!("Hello, {}!!!", self.owner)),
      _ => Ok(format!("Hello, {}.", self.owner)),
    }
  }
}

#[tokio::test]
async fn namespace_in_descriptor_name() {
  let tools = MathTools::default();
  let all = tools.tools();
  assert_eq!(all.len(), 2);
  let names: Vec<String> = all.iter().map(|t| t.descriptor().name.clone()).collect();
  assert!(names.contains(&"math_tools::add".to_string()));
  assert!(names.contains(&"math_tools::greet".to_string()));
}

#[tokio::test]
async fn descriptor_fields() {
  let tools = MathTools::default();
  let add_tool = tools.tools().into_iter().find(|t| t.descriptor().name == "math_tools::add").unwrap();
  let desc = add_tool.descriptor();
  assert_eq!(desc.description, "Adds two numbers together.");
  assert_eq!(desc.args.len(), 2);
  assert_eq!(desc.args[0].name, "a");
  assert_eq!(desc.args[0].description.as_deref(), Some("First operand"));
  assert!(desc.args[0].required);
  assert_eq!(desc.args[1].name, "b");
  assert!(desc.args[1].required);
}

#[tokio::test]
async fn handle_success() {
  let mut add_tool = MathTools::default().tools().into_iter().find(|t| t.descriptor().name == "math_tools::add").unwrap();
  let result = add_tool.handle(r#"{"a": 1, "b": 2}"#.to_string()).await;
  assert_eq!(result, "1 + 2 = 3");
}

#[tokio::test]
async fn handle_stateful() {
  let mut add_tool = MathTools { precision: 2, ..Default::default() }.tools().into_iter().find(|t| t.descriptor().name == "math_tools::add").unwrap();
  let result = add_tool.handle(r#"{"a": 1, "b": 2}"#.to_string()).await;
  assert_eq!(result, "1.00 + 2.00 = 3.00");
}

#[tokio::test]
async fn handle_parse_error() {
  let mut add_tool = MathTools::default().tools().into_iter().find(|t| t.descriptor().name == "math_tools::add").unwrap();
  let result = add_tool.handle("invalid json".to_string()).await;
  assert!(result.starts_with("Error: "));
}

#[tokio::test]
async fn optional_arg_descriptor() {
  let tools = MathTools::default();
  let greet_tool = tools.tools().into_iter().find(|t| t.descriptor().name == "math_tools::greet").unwrap();
  let desc = greet_tool.descriptor();
  assert_eq!(desc.args.len(), 1);
  assert!(!desc.args[0].required);
}

#[tokio::test]
async fn optional_arg_handle() {
  let mut greet_tool = MathTools { owner: "world".to_string(), ..Default::default() }.tools().into_iter().find(|t| t.descriptor().name == "math_tools::greet").unwrap();
  assert_eq!(greet_tool.handle(r#"{}"#.to_string()).await, "Hello, world.");
  assert_eq!(greet_tool.handle(r#"{"excited": true}"#.to_string()).await, "Hello, world!!!");
}

#[tokio::test]
async fn associated_function_skipped() {
  #[derive(Clone)]
  struct S;
  #[toolset]
  impl S {
    #[allow(unused)]
    fn static_only() -> Result<String, anyhow::Error> {
      Ok("static".to_string())
    }
  }
  let s = S;
  assert!(s.tools().is_empty());
}

#[tokio::test]
async fn no_args() {
  #[derive(Clone)]
  struct Pinger;
  #[toolset]
  impl Pinger {
    /// Returns pong.
    async fn ping(&self) -> Result<String, anyhow::Error> {
      Ok("pong".to_string())
    }
  }
  let mut tool = Pinger.tools().into_iter().next().unwrap();
  assert_eq!(tool.descriptor().name, "pinger::ping");
  assert!(tool.descriptor().args.is_empty());
  assert_eq!(tool.handle(r#"{}"#.to_string()).await, "pong");
}

#[tokio::test]
async fn casing_camel() {
  #[derive(Clone)]
  struct CamelTools;
  #[toolset(casing = "camel")]
  impl CamelTools {
    /// Multiplies two numbers.
    /// @first_operand First operand
    /// @second_operand Second operand
    async fn multiply(
      &self,
      first_operand: i64,
      second_operand: i64,
    ) -> Result<String, anyhow::Error> {
      Ok(format!("{}", first_operand * second_operand))
    }
  }
  let mut tool = CamelTools.tools().into_iter().next().unwrap();
  let desc = tool.descriptor();
  assert_eq!(desc.name, "camel_tools::multiply");
  assert_eq!(desc.args[0].name, "firstOperand");
  assert_eq!(desc.args[1].name, "secondOperand");
  assert_eq!(
    tool.handle(r#"{"firstOperand": 3, "secondOperand": 4}"#.to_string()).await,
    "12"
  );
}

#[tokio::test]
async fn namespace_acronym_struct() {
  #[derive(Clone)]
  struct HTTPTools;
  #[toolset]
  impl HTTPTools {
    /// Performs a GET request.
    async fn get(&self) -> Result<String, anyhow::Error> {
      Ok("ok".to_string())
    }
  }
  let mut tool = HTTPTools.tools().into_iter().next().unwrap();
  assert_eq!(tool.descriptor().name, "http_tools::get");
  assert_eq!(tool.handle(r#"{}"#.to_string()).await, "ok");
}

#[tokio::test]
async fn namespace_consecutive_acronym_struct() {
  #[derive(Clone)]
  struct XMLHTTPRequestHandler;
  #[toolset]
  impl XMLHTTPRequestHandler {
    /// Sends a request.
    async fn send(&self) -> Result<String, anyhow::Error> {
      Ok("sent".to_string())
    }
  }
  let tools = XMLHTTPRequestHandler;
  let tool = tools.tools().into_iter().next().unwrap();
  assert_eq!(tool.descriptor().name, "xmlhttp_request_handler::send");
}

#[tokio::test]
async fn namespace_acronym_suffix_struct() {
  #[derive(Clone)]
  struct ApiV2IOHandler;
  #[toolset]
  impl ApiV2IOHandler {
    /// Reads data.
    async fn read(&self) -> Result<String, anyhow::Error> {
      Ok("data".to_string())
    }
  }
  let tools = ApiV2IOHandler;
  let tool = tools.tools().into_iter().next().unwrap();
  assert_eq!(tool.descriptor().name, "api_v2io_handler::read");
}

#[tokio::test]
async fn mut_self() {
  #[derive(Clone)]
  struct Counter {
    count: u64,
  }
  #[toolset]
  impl Counter {
    /// Increments the counter by the given amount.
    /// @amount Amount to add
    async fn increment(&mut self, amount: u64) -> Result<String, anyhow::Error> {
      self.count += amount;
      Ok(format!("count is now {}", self.count))
    }
  }
  let mut tool = Counter { count: 0 }.tools().into_iter().next().unwrap();
  assert_eq!(tool.handle(r#"{"amount": 3}"#.to_string()).await, "count is now 3");
  assert_eq!(tool.handle(r#"{"amount": 7}"#.to_string()).await, "count is now 10");
}
