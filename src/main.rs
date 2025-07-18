use anyhow::Result;
use clap::Parser;
use kubemc::client::ListResponse;
use kubemc::commands::Cli;

pub struct TestStruct {
    pub name: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    tokio_rustls::rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("failed to install crypto provider");
    let cli = Cli::parse();

    match &cli.action {
        kubemc::commands::Action::Get { resource, name } => cli.get(resource, name).await?,
        kubemc::commands::Action::GenerateConfig => cli.generate_config().await?,
        kubemc::commands::Action::Namespace { namespace } => cli.namespace(namespace).await?,
    }

    Ok(())
}
