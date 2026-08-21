#[kiruklaw_macros::tool]
async fn bad(x: Vec<String>) -> Result<String, anyhow::Error> {
  Ok(x.join(", "))
}

fn main() {}
