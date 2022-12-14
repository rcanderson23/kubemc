use std::io::{self, Write};

use anyhow::{anyhow, Result};
use clap::{Parser, ValueEnum};

use crate::{
    client::Client,
    config::Config,
    output::{create_table, Output},
};

#[derive(Debug, Parser)]
#[clap(version, about, long_about = None)]
pub struct Cli {
    /// Action for CLI to use
    pub action: Action,

    /// Kubernetes resource to apply action to
    pub resource: Option<String>,

    // TODO implement fetching specific resources in clusters
    /// Name of resource to retrieve
    //pub name: Option<String>,

    /// Path to config file
    #[arg(long, short)]
    pub config_file: Option<String>,

    /// Namespace to fetch resources from
    #[arg(long, short)]
    pub namespace: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, ValueEnum)]
pub enum Action {
    Get,
    GenerateConfig,
    Namespace,
}

impl Cli {
    pub async fn get(&self) -> Result<()> {
        let config = Config::load_config(self.config_file.as_ref())?;
        let clusterset = config.active_clusterset()?;
        let ns = config.active_namespace()?;
        let mut clients: Vec<Client> = Vec::new();
        for cluster in &clusterset.clusters {
            let resource = self.resource.clone().unwrap_or_default();
            clients.push(Client::try_new(cluster, &ns, &resource).await?)
        }
        let outputs = list_resources(clients).await;

        create_table(outputs);
        Ok(())
    }

    pub async fn generate_config(&self) -> Result<()> {
        let config_yaml = Config::yaml()?;
        io::stdout().write(config_yaml.as_bytes()).map(|_| Ok(()))?
    }

    pub async fn namespace(&self) -> Result<()> {
        if self.resource.is_none() {
            return Err(anyhow!("namespace namespace not provided"));
        }
        let mut config = Config::load_config_from_default_file()?;
        config.set_namespace(self.resource.as_ref().unwrap().to_string())?;
        Config::write_config_to_defaul(serde_yaml::to_string(&config)?)
    }
}

// Fetch resources using all clients in parallel
async fn list_resources(api: Vec<Client>) -> Vec<Output> {
    let handles = futures::future::join_all(
        api.into_iter()
            .map(|client| tokio::spawn(async move { client.list().await })),
    )
    .await;

    let mut outputs: Vec<Output> = Vec::new();
    for handle in handles {
        let mut output_list = handle
            .unwrap_or_else(|_| Ok(Vec::new()))
            .unwrap_or_default();
        outputs.append(&mut output_list);
    }

    outputs
}
