use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::{fs, path::Path};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MCConfig {
    /// Version of multicluster config
    #[serde(rename = "apiVersion")]
    pub api_version: String,

    /// Clusterset to use by default
    #[serde(rename = "current-clusterset")]
    pub current_clusterset: String,

    /// Clustersets available to use
    pub clustersets: Vec<Clusterset>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Clusterset {
    /// Name of clusterset
    pub name: String,

    /// Clusters to query as part of the clusterset
    pub clusters: Vec<Cluster>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Cluster {
    /// The cluster to use defined in your kubeconfig
    pub cluster: String,

    /// The user to use to connect to the cluster, defined in your kubeconfig
    pub user: String,
}
impl MCConfig {
    pub fn load_config<P: AsRef<Path>>(path: P) -> Result<MCConfig> {
        let data = fs::read_to_string(path).context("failed to load file")?;
        parse_config(&data)
    }
}

fn parse_config(c: &str) -> Result<MCConfig> {
    Ok(serde_yaml::from_str(c)?)
}

mod tests {
    use super::*;

    #[test]
    fn mcconfig_deserialize() {
        let config_yaml = "apiVersion: mcconfig/v1alpha1
current-clusterset: prod
clustersets:
- name: prod
  clusters:
  - cluster: prod1
    user: prod-admin
  - cluster: prod2
    user: prod-read-only
- name: stage
  clusters:
  - cluster: stage1 
    user: stage-admin
  - cluster: stage2
    user: stage-read-only
";

        let config = parse_config(config_yaml).unwrap();
        assert_eq!(config.clustersets[0].name, "prod");
        assert_eq!(config.clustersets[1].name, "stage");
    }
}
