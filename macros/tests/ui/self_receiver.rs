struct Foo;

impl Foo {
  #[kiruklaw_macros::tool]
  async fn bad(&self) -> Result<String, anyhow::Error> {
    Ok(String::new())
  }
}

fn main() {}
