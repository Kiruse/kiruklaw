struct Foo;

impl Foo {
  #[kiruklaw_macros::tool]
  fn bad(&self) -> Result<String, anyhow::Error> {
    Ok(String::new())
  }
}

fn main() {}
