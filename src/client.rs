use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use k8s_openapi::{
    api::core::v1::{ContainerStatus, NodeCondition},
    apimachinery::pkg::apis::meta::v1::Time,
    chrono::Utc,
};
use kube::{
    api::ListParams,
    config::Kubeconfig,
    core::DynamicObject,
    discovery::{ApiCapabilities, ApiResource, Scope},
    Api, Client as KubeClient, Discovery, ResourceExt,
};
use serde::Deserialize;
use std::fmt::Display;

use crate::config::Cluster;
use crate::output::Output;

pub struct Client {
    clustername: String,
    kubeclient: Api<DynamicObject>,
}

#[async_trait]
pub trait MCClient {
    async fn get(&self, name: String) -> Result<()>;
    async fn list(&self) -> Result<Vec<Output>>;
}

impl Client {
    pub async fn try_new(
        cluster: Cluster,
        namespace: Option<String>,
        resource: &str,
    ) -> Result<Self> {
        let clustername = cluster.name.clone();
        let kubeconfig = Kubeconfig::read()?;
        let options = cluster.into();

        let config = kube::config::Config::from_custom_kubeconfig(kubeconfig, &options).await?;
        let client = KubeClient::try_from(config)?;

        let discovery = Discovery::new(client.clone())
            .run()
            .await
            .context("failed to discover api resources")?;

        let ar_cap = resolve_api_resource(&discovery, resource);

        if let Some((ar, cap)) = ar_cap {
            if cap.scope == Scope::Cluster {
                Ok(Self {
                    clustername,
                    kubeclient: Api::all_with(client, &ar),
                })
            } else if let Some(namespace) = namespace {
                Ok(Self {
                    clustername,
                    kubeclient: Api::namespaced_with(client, &namespace, &ar),
                })
            } else {
                Ok(Self {
                    clustername,
                    kubeclient: Api::default_namespaced_with(client, &ar),
                })
            }
        } else {
            Err(anyhow!("failed to find resource"))
        }
    }

    pub async fn get(&self, name: String) -> Result<()> {
        let list = self.kubeclient.get(&name).await?.name_any();
        println!("{:?}", list);
        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<Output>> {
        let list = self.kubeclient.list(&ListParams::default()).await?;
        let mut outputs: Vec<Output> = Vec::new();
        for object in list.items {
            let status: Status =
                serde_json::from_value(object.data["status"].clone()).unwrap_or_default();
            outputs.push(Output {
                cluster: self.clustername.clone(),
                namespace: object.namespace().unwrap_or_default(),
                name: object.name_any(),
                status: status.get_status(),
                ready: status.get_ready(),
                age: get_age(object.creation_timestamp()),
            });
        }
        Ok(outputs)
    }
}

fn get_age(creation: Option<Time>) -> String {
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

#[derive(Clone, Debug, Deserialize, Default)]
struct Status {
    #[serde(rename = "containerStatuses")]
    pub container_statuses: Option<Vec<ContainerStatus>>,

    pub phase: Option<String>,

    pub replicas: Option<u16>,

    // Node conditions
    pub conditions: Option<Vec<Condition>>,

    #[serde(rename = "availableReplicas")]
    pub available_replicas: Option<u16>,

    #[serde(rename = "readyReplicas")]
    pub ready_replicas: Option<u16>,

    #[serde(rename = "updatedReplicas")]
    pub updated_replicas: Option<u16>,
}

#[derive(Clone, Debug, Deserialize, Default)]
struct Condition {
    #[serde(rename = "type")]
    pub type_: String,

    pub status: String,
}

impl Status {
    fn get_ready(&self) -> String {
        if let Some(cs) = &self.container_statuses {
            let container_count = cs.len();
            let containers_ready = cs.iter().filter(|cs| cs.ready).count();
            return format!("{}/{}", containers_ready, container_count);
        }
        if let (Some(ready_rep), Some(rep)) = (&self.ready_replicas, &self.replicas) {
            return format!("{}/{}", ready_rep, rep);
        }

        String::default()
    }

    fn get_status(&self) -> String {
        if self.phase.is_some() {
            return self.phase.clone().unwrap_or_default();
        }

        match &self.conditions {
            Some(c) => {
                let mut status = String::new();
                for condition in c {
                    if condition.type_.as_str() == "Ready" {
                        status = match condition.status.as_str() {
                            "True" => String::from("Ready"),
                            "False" => String::from("NotReady"),
                            _ => String::default(),
                        }
                    }
                }
                status
            }
            None => String::default(),
        }
    }
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}  {}  {}",
            self.get_ready(),
            self.phase.to_owned().unwrap_or_default(),
            self.replicas.to_owned().unwrap_or_default(),
        )
    }
}
