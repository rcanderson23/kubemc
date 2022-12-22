use std::io::{self, Write};

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::{
    client::Client,
    config::Config,
    output::{convert_list_response_to_table, create_table},
};

#[derive(Debug, Parser)]
#[clap(version, about, long_about = None)]
pub struct Cli {
    /// Action for CLI to use
    #[command(subcommand)]
    pub action: Action,

    /// Path to config file
    #[arg(long, short, global = true)]
    pub config_file: Option<String>,

    /// Namespace to fetch resources from
    #[arg(long, short, global = true)]
    pub namespace: Option<String>,
}

#[derive(Clone, Debug, Subcommand)]
pub enum Action {
    /// Get/List Kubernetes resources
    #[command(arg_required_else_help = true)]
    Get {
        /// Kubernetes resource (pod, node, etc)
        resource: String,

        /// Name of resource
        name: Option<String>,
    },

    /// Generates an example config
    GenerateConfig,

    #[command(arg_required_else_help = true)]
    /// Changes the configured namespace in kubemc config
    Namespace { namespace: String },
}

impl Cli {
    pub async fn get(&self, resource: &str, _name: &Option<String>) -> Result<()> {
        let config = Config::load_config(self.config_file.as_ref())?;
        let clusterset = config.active_clusterset()?;
        let mut ns = config.active_namespace()?;
        if let Some(namespace) = &self.namespace {
            ns = namespace.to_owned()
        }
        let client = Client::try_new(&clusterset.clusters, &ns, resource).await?;
        let lrs = client.list().await?;

        let mut outputs = Vec::new();

        for lr in lrs {
            outputs.append(&mut convert_list_response_to_table(lr))
        }
        create_table(outputs);
        Ok(())
    }

    pub async fn generate_config(&self) -> Result<()> {
        let config_yaml = Config::yaml()?;
        io::stdout().write(config_yaml.as_bytes()).map(|_| Ok(()))?
    }

    pub async fn namespace(&self, ns: &str) -> Result<()> {
        let mut config = Config::load_config_from_default_file()?;
        config.set_namespace(ns)?;
        Config::write_config_to_defaul(serde_yaml::to_string(&config)?)
    }
}
