// SPDX-License-Identifier: MIT
use kiruklaw_agent_loop::tools::{AgentToolset, AgentToolsetMut};
use kiruklaw_macros::toolset;

#[derive(Debug, Clone, Default)]
struct MathTools {
  owner: String,
  precision: u8,
}

#[toolset(readonly)]
impl MathTools {
  /// Adds two numbers together.
  /// @a First operand
  /// @b Second operand
  async fn add(&self, a: i64, b: i64) -> Result<String, anyhow::Error> {
    if self.precision == 0 {
      return Ok(format!("{} + {} = {}", a, b, a + b));
    }
    Ok(format!(
      "{:.prec$} + {:.prec$} = {:.prec$}",
      a as f64,
      b as f64,
      (a + b) as f64,
      prec = self.precision as usize
    ))
  }

  /// Greets a person.
  /// @excited Add excitement
  async fn greet(&self, excited: Option<bool>) -> Result<String, anyhow::Error> {
    match excited {
      Some(true) => Ok(format!("Hello, {}!!!", self.owner)),
      _ => Ok(format!("Hello, {}.", self.owner)),
    }
  }
}

#[derive(Debug, Clone, Default)]
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

#[derive(Clone, Default)]
struct ReadOnlyTools {
  prefix: String,
}

#[toolset(readonly)]
impl ReadOnlyTools {
  /// Returns the prefix.
  async fn get_prefix(&self) -> Result<String, anyhow::Error> {
    Ok(self.prefix.clone())
  }
}

#[derive(Clone)]
struct AppCtx {
  user_id: u32,
}

#[derive(Clone)]
struct CtxTools;

#[toolset(ctx = AppCtx)]
impl CtxTools {
  /// Returns the user id from context.
  async fn get_user_id(&self, ctx: &AppCtx) -> Result<String, anyhow::Error> {
    Ok(format!("{}", ctx.user_id))
  }

  /// Returns a greeting.
  /// @name The name to greet
  async fn greet_user(&self, ctx: &AppCtx, name: String) -> Result<String, anyhow::Error> {
    Ok(format!("user {} says hi {}", ctx.user_id, name))
  }
}

#[tokio::test]
async fn name_and_descriptors() {
  let tools = MathTools::default();
  assert_eq!(tools.name(), "math_tools");
  let descs = tools.tools();
  assert_eq!(descs.len(), 2);
  let names: Vec<String> = descs.iter().map(|d| d.name.clone()).collect();
  assert!(names.contains(&"math_tools::add".to_string()));
  assert!(names.contains(&"math_tools::greet".to_string()));
}

#[tokio::test]
async fn descriptor_fields() {
  let tools = MathTools::default();
  let desc = tools
    .tools()
    .into_iter()
    .find(|d| d.name == "math_tools::add")
    .unwrap();
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
  let tools = MathTools::default();
  let result = tools.handle(&(), "add", r#"{"a": 1, "b": 2}"#.to_string()).await;
  assert_eq!(result, "1 + 2 = 3");
}

#[tokio::test]
async fn handle_stateful() {
  let tools = MathTools {
    precision: 2,
    ..Default::default()
  };
  let result = tools.handle(&(), "add", r#"{"a": 1, "b": 2}"#.to_string()).await;
  assert_eq!(result, "1.00 + 2.00 = 3.00");
}

#[tokio::test]
async fn handle_parse_error() {
  let tools = MathTools::default();
  let result = tools
    .handle(&(), "math_tools::add", "invalid json".to_string())
    .await;
  assert!(result.starts_with("Error: "));
}

#[tokio::test]
async fn handle_unknown_tool() {
  let tools = MathTools::default();
  let result = tools
    .handle(&(), "nonexistent", r#"{}"#.to_string())
    .await;
  assert_eq!(result, "Error: unknown tool nonexistent");
}

#[tokio::test]
async fn optional_arg_descriptor() {
  let tools = MathTools::default();
  let desc = tools
    .tools()
    .into_iter()
    .find(|d| d.name == "math_tools::greet")
    .unwrap();
  assert_eq!(desc.args.len(), 1);
  assert!(!desc.args[0].required);
}

#[tokio::test]
async fn optional_arg_handle() {
  let tools = MathTools {
    owner: "world".to_string(),
    ..Default::default()
  };
  assert_eq!(
    tools.handle(&(), "greet", r#"{}"#.to_string()).await,
    "Hello, world."
  );
  assert_eq!(
    tools
      .handle(&(), "greet", r#"{"excited": true}"#.to_string())
      .await,
    "Hello, world!!!"
  );
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
  let mut p = Pinger;
  assert_eq!(p.name(), "pinger");
  let descs = p.tools();
  assert_eq!(descs.len(), 1);
  assert_eq!(descs[0].name, "pinger::ping");
  assert!(descs[0].args.is_empty());
  assert_eq!(p.handle(&(), "ping", r#"{}"#.to_string()).await, "pong");
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
  let mut ct = CamelTools;
  let descs = ct.tools();
  assert_eq!(descs[0].name, "camelTools::multiply");
  assert_eq!(descs[0].args[0].name, "firstOperand");
  assert_eq!(descs[0].args[1].name, "secondOperand");
  assert_eq!(
    ct.handle(
      &(),
      "multiply",
      r#"{"firstOperand": 3, "secondOperand": 4}"#.to_string()
    )
    .await,
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
  let mut t = HTTPTools;
  assert_eq!(t.name(), "http_tools");
  assert_eq!(t.tools()[0].name, "http_tools::get");
  assert_eq!(t.handle(&(), "get", r#"{}"#.to_string()).await, "ok");
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
  let t = XMLHTTPRequestHandler;
  assert_eq!(t.name(), "xmlhttp_request_handler");
  assert_eq!(t.tools()[0].name, "xmlhttp_request_handler::send");
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
  let t = ApiV2IOHandler;
  assert_eq!(t.name(), "api_v2io_handler");
  assert_eq!(t.tools()[0].name, "api_v2io_handler::read");
}

#[tokio::test]
async fn mut_self() {
  let mut c = Counter::default();
  assert_eq!(
    c.handle(&(), "increment", r#"{"amount": 3}"#.to_string()).await,
    "count is now 3"
  );
  assert_eq!(
    c.handle(&(), "increment", r#"{"amount": 7}"#.to_string()).await,
    "count is now 10"
  );
  assert_eq!(c.count, 10);
}

#[tokio::test]
async fn readonly_toolset() {
  let t = ReadOnlyTools {
    prefix: "hello".to_string(),
  };
  assert_eq!(t.name(), "read_only_tools");
  let descs = t.tools();
  assert_eq!(descs.len(), 1);
  assert_eq!(descs[0].name, "read_only_tools::get_prefix");
  assert_eq!(
    t.handle(&(), "get_prefix", r#"{}"#.to_string())
      .await,
    "hello"
  );
}

#[tokio::test]
async fn readonly_handle_ref() {
  let t = ReadOnlyTools {
    prefix: "world".to_string(),
  };
  let result = t
    .handle(&(), "get_prefix", r#"{}"#.to_string())
    .await;
  assert_eq!(result, "world");
}

#[tokio::test]
async fn toolset_enum_dispatch() {
  use kiruklaw_agent_loop::tools::Toolset;
  let mut ts: Toolset = MathTools {
    owner: "test".to_string(),
    ..Default::default()
  }.to_toolset();
  assert_eq!(ts.name(), "math_tools");
  assert_eq!(ts.tools().len(), 2);
  assert_eq!(
    ts.handle(&(), "greet", r#"{"excited": true}"#.to_string()).await,
    "Hello, test!!!"
  );

  let mut ts = Counter::default().to_toolset();
  assert_eq!(ts.name(), "counter");
  assert_eq!(
    ts.handle(&(), "increment", r#"{"amount": 4}"#.to_string()).await,
    "count is now 4"
  );
  assert_eq!(
    ts.handle(&(), "increment", r#"{"amount": 2}"#.to_string()).await,
    "count is now 6"
  );
}

#[tokio::test]
async fn ctx_toolset() {
  let mut t = CtxTools;
  assert_eq!(t.name(), "ctx_tools");
  let descs = t.tools();
  assert_eq!(descs.len(), 2);
  assert_eq!(descs[0].name, "ctx_tools::get_user_id");
  assert_eq!(descs[1].name, "ctx_tools::greet_user");

  let ctx = AppCtx { user_id: 42 };
  assert_eq!(
    t.handle(&ctx, "get_user_id", r#"{}"#.to_string()).await,
    "42"
  );
  assert_eq!(
    t.handle(&ctx, "greet_user", r#"{ "name": "alice" }"#.to_string()).await,
    "user 42 says hi alice"
  );
}

#[tokio::test]
async fn ctx_toolset_to_toolset() {
  use kiruklaw_agent_loop::tools::Toolset;
  let mut ts: Toolset<AppCtx> = CtxTools.to_toolset();
  let ctx = AppCtx { user_id: 99 };
  assert_eq!(ts.name(), "ctx_tools");
  assert_eq!(
    ts.handle(&ctx, "get_user_id", r#"{}"#.to_string()).await,
    "99"
  );
}
