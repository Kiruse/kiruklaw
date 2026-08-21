use std::{fmt::Debug, future::Future, pin::Pin};

use serde_json::{Map, Value, json};

pub trait AgentToolset<C = ()>: Send + Sync {
  /// Get this toolset's name.
  fn name(&self) -> &'static str;
  /// Get a collection of this toolset's tools' descriptors. These
  /// descriptors will include the toolset's name as namespace.
  fn tools(&self) -> Vec<AgentToolDescriptor>;
  /// Execute the given named tool, without toolset namespace.
  fn handle<'a>(&'a self, ctx: &'a C, tool_name: &str, args: String) -> Pin<Box<dyn Future<Output = String> + Send + 'a>>;
}

pub trait AgentToolsetMut<C = ()>: Send + Sync {
  /// Get this toolset's name.
  fn name(&self) -> &'static str;
  /// Get a collection of this toolset's tools' descriptors. These
  /// descriptors will include the toolset's name as namespace.
  fn tools(&self) -> Vec<AgentToolDescriptor>;
  /// Execute the given named tool, without toolset namespace.
  fn handle<'a>(&'a mut self, ctx: &'a C, tool_name: &str, args: String) -> Pin<Box<dyn Future<Output = String> + Send + 'a>>;
}

pub enum Toolset<C = ()> {
  Immutable(Box<dyn AgentToolset<C>>),
  Mutable(Box<dyn AgentToolsetMut<C>>),
}

impl<C> Toolset<C> {
  pub fn name(&self) -> &'static str {
    match self {
      Self::Immutable(t) => t.name(),
      Self::Mutable(t) => t.name(),
    }
  }

  pub fn tools(&self) -> Vec<AgentToolDescriptor> {
    match self {
      Self::Immutable(t) => t.tools(),
      Self::Mutable(t) => t.tools(),
    }
  }

  pub fn handle<'a>(&'a mut self, ctx: &'a C, tool_name: &str, args: String) -> Pin<Box<dyn Future<Output = String> + Send + 'a>> {
    match self {
      Self::Immutable(t) => t.handle(ctx, tool_name, args),
      Self::Mutable(t) => t.handle(ctx, tool_name, args),
    }
  }
}

#[derive(Debug, Clone)]
pub struct AgentToolDescriptor {
  pub name: String,
  pub description: String,
  pub args: Vec<AgentToolArg>,
}

impl AgentToolDescriptor {
  pub fn new(name: impl Into<String>, desc: impl Into<String>, args: Vec<AgentToolArg>) -> Self {
    Self {
      name: name.into(),
      description: desc.into(),
      args,
    }
  }

  /// Get a JSON schema representation of this agent tool definition.
  pub fn to_schema(&self) -> Value {
    let name = &self.name;
    let desc = &self.description;

    let props = json!(
      self
        .args
        .iter()
        .map(|arg| {
          let mut val = Map::new();
          arg.ty.apply(&mut val);
          if let Some(desc) = &arg.description {
            val["description"] = Value::String(desc.clone());
          }
          (arg.name.clone(), json!(val))
        })
        .collect::<Map<_, _>>()
    );

    let required = self
      .args
      .iter()
      .filter(|arg| arg.required)
      .map(|arg| arg.name.clone())
      .collect::<Vec<_>>();

    let params = json!({
      "type": "object",
      "properties": props,
      "required": required,
    });

    json!({
      "type": "function",
      "function": {
        "name": name,
        "description": desc,
        "parameters": params,
      },
    })
  }
}

#[derive(Debug, Clone)]
pub struct AgentToolArg {
  pub name: String,
  pub ty: AgentToolArgType,
  pub description: Option<String>,
  pub required: bool,
}

impl AgentToolArg {
  pub fn new(name: impl Into<String>, ty: AgentToolArgType) -> Self {
    Self {
      name: name.into(),
      ty,
      description: None,
      required: false,
    }
  }

  pub fn with_description(self, desc: String) -> Self {
    Self {
      description: Some(desc),
      ..self
    }
  }

  pub fn required(self) -> Self {
    Self {
      required: true,
      ..self
    }
  }
}

#[derive(Debug, Clone)]
pub enum AgentToolArgType {
  Primitive(AgentToolArgPrimitive),
  Enum(AgentToolArgEnum),
}

impl AgentToolArgType {
  pub fn apply(&self, value: &mut Map<String, Value>) {
    match self {
      Self::Primitive(ty) => {
        value["type"] = ty.to_schema();
      }
      Self::Enum(ty) => {
        value["type"] = json!(ty.type_string());
        value["enum"] = ty.to_schema();
      }
    }
  }
}

#[derive(Debug, Copy, Clone)]
pub struct AgentToolArgPrimitive(pub u8);

impl AgentToolArgPrimitive {
  pub const NULL: Self = Self(1);
  pub const STRING: Self = Self(2);
  pub const NUMBER: Self = Self(3);
  pub const INT: Self = Self(4);
  pub const BOOL: Self = Self(5);

  #[inline(always)]
  pub fn new() -> Self {
    Self(0)
  }

  #[inline(always)]
  pub fn nullable(self) -> Self {
    Self(self.0 | 1)
  }

  #[inline(always)]
  pub fn with_string(self) -> Self {
    Self(self.0 | 2)
  }

  #[inline(always)]
  pub fn with_number(self) -> Self {
    Self(self.0 | 3)
  }

  #[inline(always)]
  pub fn with_integer(self) -> Self {
    Self(self.0 | 4)
  }

  #[inline(always)]
  pub fn with_boolean(self) -> Self {
    Self(self.0 | 5)
  }

  #[inline(always)]
  pub fn is_nullable(&self) -> bool {
    self.0 & Self::NULL.0 != 0
  }

  #[inline(always)]
  pub fn is_string(&self) -> bool {
    self.0 & Self::STRING.0 != 0
  }

  #[inline(always)]
  pub fn is_number(&self) -> bool {
    self.0 & Self::NUMBER.0 != 0
  }

  #[inline(always)]
  pub fn is_int(&self) -> bool {
    self.0 & Self::INT.0 != 0
  }

  #[inline(always)]
  pub fn is_bool(&self) -> bool {
    self.0 & Self::BOOL.0 != 0
  }

  /// Get a JSON schema representation of this type set
  pub fn to_schema(&self) -> Value {
    let is_union = self.0.count_ones() != 1;
    let mut types = [
      (Self::NULL, "null"),
      (Self::STRING, "string"),
      (Self::NUMBER, "number"),
      (Self::INT, "integer"),
      (Self::BOOL, "boolean"),
    ]
    .into_iter()
    .filter(|(r, _)| self.0 & r.0 != 0)
    .map(|(_, s)| Value::String(s.to_string()))
    .collect::<Vec<_>>();
    if !is_union {
      types.pop().unwrap()
    } else {
      Value::Array(types)
    }
  }
}

impl Into<AgentToolArgType> for AgentToolArgPrimitive {
  fn into(self) -> AgentToolArgType {
    AgentToolArgType::Primitive(self)
  }
}

#[derive(Debug, Clone)]
pub enum AgentToolArgEnum {
  String(Vec<String>),
  Integer(Vec<i64>),
}

impl AgentToolArgEnum {
  pub fn string(values: Vec<String>) -> Self {
    Self::String(values)
  }

  pub fn ints(values: Vec<i64>) -> Self {
    Self::Integer(values)
  }

  pub fn type_string(&self) -> &'static str {
    match self {
      Self::String(_) => "string",
      Self::Integer(_) => "integer",
    }
  }

  pub fn to_schema(&self) -> Value {
    match self {
      Self::String(values) => Value::Array(values.iter().map(|arg| json!(arg)).collect()),
      Self::Integer(values) => Value::Array(values.iter().map(|arg| json!(arg)).collect()),
    }
  }
}

impl Into<AgentToolArgType> for AgentToolArgEnum {
  fn into(self) -> AgentToolArgType {
    AgentToolArgType::Enum(self)
  }
}
