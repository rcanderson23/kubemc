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
    /// Return default config file in yaml format
    pub fn yaml() -> Result<String> {
        let cluster = Cluster {
            name: "cluster1".into(),
            cluster: Some("CLUSTER".into()),
            user: Some("USER".into()),
            context: None,
        };

        let clusterset = Clusterset {
            name: "clusterset1".into(),
            namespace: "default".into(),
            clusters: vec![cluster],
        };

        let config = Config {
            api_version: "kubemc/v1alpha1".into(),
            current_clusterset: "clusterset1".into(),
            clustersets: vec![clusterset],
        };

        let config_yaml = serde_yaml::to_string(&config)?;
        Ok(config_yaml)
    }

    pub fn active_clusterset(&self) -> Result<&Clusterset> {
        for clusterset in &self.clustersets {
            if clusterset.name == self.current_clusterset {
                return Ok(clusterset);
            }
        }
        Err(anyhow!("clusterset {} not found", self.current_clusterset))
    }

    pub fn active_namespace(&self) -> Result<String> {
        match self.active_clusterset() {
            Ok(cs) => Ok(cs.namespace.clone()),
            Err(e) => Err(e),
        }
    }

    pub fn set_namespace(&mut self, ns: &str) -> Result<()> {
        for mut clusterset in &mut self.clustersets {
            if clusterset.name == self.current_clusterset {
                clusterset.namespace = ns.to_owned();
                return Ok(());
            }
        }

        Err(anyhow!("failed to find active cluster"))
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

    pub fn load_config_from_default_file() -> Result<Config> {
        let path = default_config_path().unwrap_or_default();
        let data = fs::read_to_string(path).context("failed to load file")?;
        parse_config(&data)
    }

    pub fn write_config_to_defaul(config: String) -> Result<()> {
        let path = default_config_path().unwrap_or_default();
        fs::write(path, config).context("failed to write kubemc config")
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Clusterset {
    /// Name of clusterset
    pub name: String,

    /// Active namespace for namespaced objects
    pub namespace: String,

    /// Clusters to query as part of the clusterset
    pub clusters: Vec<Cluster>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct Cluster {
    /// The name used to associate cluster output with
    pub name: String,

    /// The cluster to use defined in your kubeconfig
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster: Option<String>,

    /// The user to use to connect to the cluster, defined in your kubeconfig
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    /// Allow users to specify a context rather than both the cluster and user
    #[serde(skip_serializing_if = "Option::is_none")]
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

impl From<&Cluster> for KubeConfigOptions {
    fn from(c: &Cluster) -> Self {
        Self {
            context: c.context.clone(),
            cluster: c.cluster.clone(),
            user: c.user.clone(),
        }
    }
}

fn parse_config(c: &str) -> Result<Config> {
    Ok(serde_yaml::from_str(c)?)
}

fn default_config_path() -> Option<PathBuf> {
    home_dir().map(|h| h.join(".kube").join("kubemc"))
}

fn env_config_path() -> Option<PathBuf> {
    let path = std::env::var("KUBEMC_CONFIG");
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
