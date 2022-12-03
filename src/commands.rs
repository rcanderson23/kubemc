use std::io::{self, Write};

use anyhow::{anyhow, Result};
use clap::{Parser, ValueEnum};
use kube::{
    config::{KubeConfigOptions, Kubeconfig},
    core::DynamicObject,
    discovery::{ApiCapabilities, ApiResource, Scope},
    Api, Client, Discovery,
};

use crate::config::{Cluster, MCConfig};

#[derive(Debug, Parser)]
#[clap(version, about, long_about = None)]
pub struct Cli {
    /// Action for CLI to use
    pub action: Action,

    /// Kubernetes resource to apply action to
    pub resource: Option<String>,

    /// Path to config file
    pub config_file: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, ValueEnum)]
pub enum Action {
    Get,
    GenerateConfig,
}

impl Cli {
    pub async fn get(&self) -> Result<()> {
        let clusterset = MCConfig::load_config(self.config_file.as_ref())?.active_clusterset()?;
        let mut clients: Vec<Api<DynamicObject>> = Vec::new();
        for cluster in clusterset.clusters {
            let resource = self.resource.clone().unwrap_or_else(|| "".to_string());
            clients.push(create_cluster_client(cluster, None, &resource).await?)
        }
        for client in clients {
            println!("{:?}", client.get("").await?);
        }
        Ok(())
    }

    pub async fn generate_config(&self) -> Result<()> {
        let config_yaml = MCConfig::yaml()?;
        io::stdout().write(config_yaml.as_bytes()).map(|_| Ok(()))?
    }
}

fn resolve_api_resource(
    discovery: &Discovery,
    name: &str,
) -> Option<(ApiResource, ApiCapabilities)> {
    // iterate through groups to find matching kind/plural names at recommended versions
    // and then take the minimal match by group.name (equivalent to sorting groups by group.name).
    // this is equivalent to kubectl's api group preference
    discovery
        .groups()
        .flat_map(|group| {
            group
                .resources_by_stability()
                .into_iter()
                .map(move |res| (group, res))
        })
        .filter(|(_, (res, _))| {
            // match on both resource name and kind name
            // ideally we should allow shortname matches as well
            name.eq_ignore_ascii_case(&res.kind) || name.eq_ignore_ascii_case(&res.plural)
        })
        .min_by_key(|(group, _res)| group.name())
        .map(|(_, res)| res)
}

async fn create_cluster_client(
    cluster: Cluster,
    ns: Option<String>,
    resource: &str,
) -> Result<Api<DynamicObject>> {
    let kubeconfig = Kubeconfig::read()?;
    let options = KubeConfigOptions {
        context: None,
        cluster: Some(cluster.cluster),
        user: Some(cluster.user),
    };

    let config = kube::config::Config::from_custom_kubeconfig(kubeconfig, &options).await?;
    let client = Client::try_from(config)?;

    let discovery = Discovery::new(client.clone()).run().await?;

    let ar_cap = resolve_api_resource(&discovery, resource);

    if let Some((ar, cap)) = ar_cap {
        if cap.scope == Scope::Cluster {
            Ok(Api::all_with(client, &ar))
        } else if let Some(namespace) = ns {
            Ok(Api::namespaced_with(client, &namespace, &ar))
        } else {
            Ok(Api::default_namespaced_with(client, &ar))
        }
    } else {
        Err(anyhow!("failed to find resource"))
    }
}
