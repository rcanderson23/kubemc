use anyhow::Result;
use clap::Parser;
use kubemc::commands::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.action {
        kubemc::commands::Action::Get{ref resource} => cli.get(resource).await?,
        kubemc::commands::Action::GenerateConfig => cli.generate_config().await?,
        kubemc::commands::Action::Namespace{ref namespace} => cli.namespace(namespace).await?,
    }
    Ok(())
}
