use anyhow::Context;
use anyhow::{anyhow, Result};
use dirs::home_dir;
use kube::config::KubeConfigOptions;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;
use std::{fs, path::Path};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// Version of multicluster config
    #[serde(rename = "apiVersion")]
    pub api_version: String,

    /// Clusterset to use by default
    #[serde(rename = "current-clusterset")]
    pub current_clusterset: String,

    /// Clustersets available to use
    pub clustersets: Vec<Clusterset>,
}

impl Config {
    /// Return defautl config file in yaml format
    pub fn yaml() -> Result<String> {
        let config = Config::default();
        let config_yaml = serde_yaml::to_string(&config)?;
        Ok(config_yaml)
    }

    pub fn active_clusterset(&self) -> Result<Clusterset> {
        for clusterset in &self.clustersets {
            if clusterset.name == self.current_clusterset {
                return Ok(clusterset.clone());
            }
        }
        Err(anyhow!("clusterset {} not found", self.current_clusterset))
    }

    /// Load from specified path, then environment variable, or finally default location
    pub fn load_config<P: AsRef<Path>>(path: Option<P>) -> Result<Config> {
        if let Some(path) = path {
            let data = fs::read_to_string(path).context("failed to load file")?;
            parse_config(&data)
        } else if let Some(path) = env_config_path() {
            let data = fs::read_to_string(path).context("failed to load file")?;
            parse_config(&data)
        } else if let Some(path) = default_config_path() {
            let data = fs::read_to_string(path).context("failed to load file")?;
            parse_config(&data)
        } else {
            Err(anyhow!("failed to load config"))
        }
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
    /// The name used to associate cluster output with
    pub name: String,

    /// The cluster to use defined in your kubeconfig
    pub cluster: Option<String>,

    /// The user to use to connect to the cluster, defined in your kubeconfig
    pub user: Option<String>,

    /// Allow users to specify a context rather than both the cluster and user
    pub context: Option<String>,
}

impl From<Cluster> for KubeConfigOptions {
    fn from(c: Cluster) -> Self {
        Self {
            context: c.context,
            cluster: c.cluster,
            user: c.user,
        }
    }
}

fn parse_config(c: &str) -> Result<Config> {
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

impl Default for Config {
    fn default() -> Self {
        Self {
            api_version: "kubemc/v1alpha1".into(),
            current_clusterset: "".into(),
            clustersets: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_deserialize() {
        let config_yaml = "apiVersion: kubemc/v1alpha1
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
    fn config_generate() {
        let config_yaml = Config::yaml().unwrap();
        assert_eq!(
            config_yaml,
            "apiVersion: mcconfig/v1alpha1
current-clusterset: ''
clustersets: []
"
        )
    }
}
