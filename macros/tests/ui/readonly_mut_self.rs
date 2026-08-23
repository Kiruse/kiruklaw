#[derive(Clone)]
struct S;

#[kiruklaw_macros::toolset]
impl AgentToolset<()> for S {
  async fn bad(&mut self) -> Result<String, anyhow::Error> {
    Ok(String::new())
  }
}

fn main() {}