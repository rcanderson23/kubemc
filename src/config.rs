use anyhow::Context;
use anyhow::Result;
use dirs::home_dir;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;
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

impl MCConfig {
    /// Return defautl config file in yaml format
    pub fn yaml() -> Result<String> {
        let config = MCConfig::default();
        let config_yaml = serde_yaml::to_string(&config)?;
        Ok(config_yaml)
    }

    /// Load from specified path, then environment variable, or finally default location
    pub fn load_config<P: AsRef<Path>>(path: P) -> Result<MCConfig> {
        let data = fs::read_to_string(path).context("failed to load file")?;
        parse_config(&data)
    }
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

fn parse_config(c: &str) -> Result<MCConfig> {
    Ok(serde_yaml::from_str(c)?)
}

fn default_config_path() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".kube").join("mcconfig"))
}

fn env_config_path() -> Option<PathBuf> {
    let path = std::env::var("MCCONFIG");
    if let Ok(p) = path {
        Some(PathBuf::from(p))
    } else {
        None
    }
}

impl Default for MCConfig {
    fn default() -> Self {
        Self {
            api_version: "mcconfig/v1alpha1".into(),
            current_clusterset: "".into(),
            clustersets: Default::default(),
        }
    }
}

#[cfg(test)]
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

    #[test]
    fn mcconfig_generate() {
        let config_yaml = MCConfig::yaml().unwrap();
        assert_eq!(
            config_yaml,
            "apiVersion: mcconfig/v1alpha1
current-clusterset: ''
clustersets: []
"
        )
    }
}
