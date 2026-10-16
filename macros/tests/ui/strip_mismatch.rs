struct StoryWidget;

#[kiruklaw_macros::toolset(strip_suffix = "module")]
impl kiruklaw_agent_loop::tools::AgentToolset<()> for StoryWidget {
  /// Reads a story.
  async fn read(&self) -> Result<String, anyhow::Error> {
    Ok("once upon a time".to_string())
  }
}

fn main() {}
