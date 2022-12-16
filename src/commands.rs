use std::io::{self, Write};

use anyhow::Result;
use clap::{Parser, Subcommand};
use k8s_openapi::{apimachinery::pkg::apis::meta::v1::Time, chrono::Utc};
use kube::ResourceExt;

use crate::{
    client::{Client, Status},
    config::Config,
    output::{create_table, Output},
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
        let client_build = tokio::time::Instant::now();
        let client = Client::try_new(&clusterset.clusters, &ns, resource).await?;
        println!("Time to build client {:?}", client_build.elapsed());
        let lrs = client.list().await?;

        let build_output = tokio::time::Instant::now();

        let mut outputs: Vec<Output> = Vec::new();
        for lr in lrs {
            let cn = lr.clustername;
            for obj in lr.object_list {
                let status: Status =
                    serde_json::from_value(obj.data["status"].to_owned()).unwrap_or_default();
                outputs.push(Output {
                    cluster: cn.to_owned(),
                    namespace: obj.namespace().unwrap_or_default(),
                    name: obj.name_any(),
                    status: status.get_status(),
                    ready: status.get_ready(),
                    age: get_age(obj.creation_timestamp()),
                });
            }
        }
        println!("Time to build output {:?}", build_output.elapsed());
        let table = tokio::time::Instant::now();
        create_table(outputs);
        println!("Time to build table {:?}", table.elapsed());
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

fn get_age(creation: Option<Time>) -> String {
    if creation.is_none() {
        return String::default();
    }
    let duration = Utc::now().signed_duration_since(creation.unwrap().0);
    match (
        duration.num_days(),
        duration.num_hours(),
        duration.num_minutes(),
        duration.num_seconds(),
    ) {
        (days, hours, _, _) if days > 2 => format!("{}d{}h", days, hours - 24 * days),
        (_, hours, mins, _) if hours > 0 => format!("{}h{}m", hours, mins - 60 * hours),
        (_, _, mins, secs) if mins > 0 => format!("{}m{}s", mins, secs - 60 * mins),
        (_, _, _, secs) => format!("{}s", secs),
    }
}
