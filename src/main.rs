mod config;

use config::Config;

fn main() -> anyhow::Result<()> {
    let config = Config::load()?;
    println!(
        "corpo-metrics-chatbot: loaded config (model={}, mcp={})",
        config.model, config.mcp_server_url
    );
    Ok(())
}
