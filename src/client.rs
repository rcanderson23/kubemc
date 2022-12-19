use anyhow::{anyhow, Context, Result};
use k8s_openapi::api::core::v1::ContainerStatus;
use kube::{
    api::ListParams,
    config::{KubeConfigOptions, Kubeconfig},
    core::{DynamicObject, ObjectList},
    discovery::{ApiCapabilities, ApiResource, Scope},
    Api, Client as KubeClient, Discovery as KubeDiscovery,
};
use serde::Deserialize;
use std::{fmt::Display, sync::Arc};
use tracing::log::{debug, warn};

use crate::{config::Cluster, discovery::Discovery};

type ClusterName = String;
type MCCluster = (ClusterName, Api<DynamicObject>);

pub struct Client {
    kubeclients: Vec<MCCluster>,
}

pub struct ListResponse {
    pub clustername: String,
    pub object_list: ObjectList<DynamicObject>,
}

impl Client {
    pub async fn try_new(clusters: &[Cluster], namespace: &str, resource: &str) -> Result<Self> {
        let kubeconfig = Kubeconfig::read()?;
        let handles = futures::future::join_all(clusters.iter().map(|cluster| {
            let kubeconfig = kubeconfig.clone();
            let cluster = cluster.clone();
            let ns = Arc::new(namespace.to_owned());
            let r = Arc::new(resource.to_owned());
            tokio::spawn(async move {
                create_client(kubeconfig, cluster, &ns.clone(), &r.clone()).await
            })
        }))
        .await;
        let mut kubeclients: Vec<MCCluster> = Vec::new();
        for handle in handles {
            match handle {
                Ok(Ok(mcclient)) => kubeclients.push(mcclient),
                Ok(Err(e)) => warn!("failed to create client {}", e),
                Err(e) => debug!("join failed {}", e),
            }
        }
        Ok(Client { kubeclients })
    }

    pub async fn list(self) -> Result<Vec<ListResponse>> {
        Ok(list_resources(self, &ListParams::default()).await)
    }
}

async fn create_client(
    kubeconfig: Kubeconfig,
    cluster: Cluster,
    namespace: &str,
    resource: &str,
) -> Result<MCCluster> {
    let clustername = cluster.name.clone();
    let options = cluster.into();

    let discovery = Discovery::new_from_default_cache(get_cluster_endpoint(&kubeconfig, &options)?);
    let config = kube::config::Config::from_custom_kubeconfig(kubeconfig, &options).await?;
    let client = KubeClient::try_from(config)?;

    // if cached discovery succeeded and the requested resource is present, use it to make the
    // request. Otherwise fall back to discovery via k8s api.
    if let Ok(discovery) = discovery {
        if let Ok((resource, scope)) = discovery.get_resource_from_name(resource) {
            debug!(
                "creating client for cluster {} for resource {} with scope {:?}",
                &clustername, &resource.kind, &scope
            );
            let client = create_typed_kubeclient(client, resource, scope, namespace);
            return Ok((clustername, client));
        }
    }

    let kube_discovery = KubeDiscovery::new(client.clone())
        .run()
        .await
        .context("failed to discover api resources")?;

    let ar_cap = resolve_api_resource(&kube_discovery, resource);

    if let Some((ar, cap)) = ar_cap {
        let client = create_typed_kubeclient(client, ar, cap.scope, namespace);
        Ok((clustername, client))
    } else {
        Err(anyhow!(
            "discovery of resource {} failed for cluster {}",
            resource,
            clustername
        ))
    }
}

fn get_cluster_endpoint(kubeconfig: &Kubeconfig, options: &KubeConfigOptions) -> Result<String> {
    if let Some(cluster) = &options.cluster {
        get_server_endpoint_from_kubeconfig(kubeconfig, cluster)
    } else if let Some(ctx) = &options.context {
        let cluster = get_cluster_from_context(kubeconfig, ctx)?;
        get_server_endpoint_from_kubeconfig(kubeconfig, &cluster)
    } else {
        Err(anyhow!("failed to find cluster"))
    }
}

// Returns the cluster name from the specified context
fn get_cluster_from_context(kubeconfig: &Kubeconfig, ctx: &str) -> Result<String> {
    kubeconfig
        .contexts
        .iter()
        .find(|named_context| named_context.name == ctx)
        .and_then(|named_context| named_context.context.clone())
        .map(|context| context.cluster)
        .ok_or_else(|| anyhow!("failed to find context {} in kubeconfig", ctx))
}
// Returns the server endpoint from a kubeconfig given a cluster
fn get_server_endpoint_from_kubeconfig(
    kubeconfig: &Kubeconfig,
    cluster_name: &str,
) -> Result<String> {
    kubeconfig
        .clusters
        .iter()
        .find(|named_cluster| named_cluster.name == cluster_name)
        .and_then(|name_cluster| name_cluster.cluster.clone())
        .and_then(|cluster| cluster.server)
        .ok_or_else(|| {
            anyhow!(
                "failed to get cluster endpoint for cluster {}",
                cluster_name
            )
        })
}

// Fetch resources using all clients in parallel
async fn list_resources(client: Client, lp: &ListParams) -> Vec<ListResponse> {
    let handles = futures::future::join_all(client.kubeclients.into_iter().map(|client| {
        let lp = lp.clone();
        tokio::spawn(async move {
            let response = client.1.list(&lp).await;
            (client.0, response)
        })
    }))
    .await;

    let mut lr: Vec<ListResponse> = Vec::new();
    for handle in handles {
        match handle {
            Ok(h) => {
                if let Ok(object_list) = h.1 {
                    lr.push(ListResponse {
                        clustername: h.0,
                        object_list,
                    })
                } else {
                    warn!("failed request to cluster {}", h.0)
                }
            }
            Err(e) => {
                debug!("join handle failed {}", e)
            }
        }
    }
    lr
}

fn create_typed_kubeclient(
    client: KubeClient,
    ar: ApiResource,
    scope: Scope,
    ns: &str,
) -> Api<DynamicObject> {
    if scope == Scope::Cluster {
        Api::all_with(client, &ar)
    } else {
        Api::namespaced_with(client, ns, &ar)
    }
}

#[allow(unused)]
// Check for commonly used resources and short names before using discovery api
fn known_resources(resource: &str) -> Option<(ApiResource, Scope)> {
    match resource {
        "po" | "pod" | "pods" => Some((
            ApiResource {
                group: "".into(),
                version: "v1".into(),
                api_version: "v1".into(),
                kind: "Pod".into(),
                plural: "pods".into(),
            },
            Scope::Namespaced,
        )),
        "no" | "node" | "nodes" => Some((
            ApiResource {
                group: "".into(),
                version: "v1".into(),
                api_version: "v1".into(),
                kind: "Node".into(),
                plural: "nodes".into(),
            },
            Scope::Cluster,
        )),
        "cm" | "configmap" | "configmaps" => Some((
            ApiResource {
                group: "".into(),
                version: "v1".into(),
                api_version: "v1".into(),
                kind: "ConfigMap".into(),
                plural: "configmaps".into(),
            },
            Scope::Namespaced,
        )),
        "deploy" | "deployment" | "deployments" => Some((
            ApiResource {
                group: "apps".into(),
                version: "v1".into(),
                api_version: "apps/v1".into(),
                kind: "Deployment".into(),
                plural: "deployments".into(),
            },
            Scope::Namespaced,
        )),
        "ds" | "daemonset" | "daemonsets" => Some((
            ApiResource {
                group: "apps".into(),
                version: "v1".into(),
                api_version: "apps/v1".into(),
                kind: "DaemonSet".into(),
                plural: "daemonsets".into(),
            },
            Scope::Namespaced,
        )),
        "pvc" | "persistentvolumeclaim" | "persistentvolumeclaims" => Some((
            ApiResource {
                group: "".into(),
                version: "v1".into(),
                api_version: "v1".into(),
                kind: "PersistentVolumeClaim".into(),
                plural: "PersistentVolumeClaims".into(),
            },
            Scope::Namespaced,
        )),
        "rs" | "replicaset" | "replicasets" => Some((
            ApiResource {
                group: "apps".into(),
                version: "v1".into(),
                api_version: "apps/v1".into(),
                kind: "ReplicaSet".into(),
                plural: "replicasets".into(),
            },
            Scope::Namespaced,
        )),
        "sts" | "statefulset" | "statefulsets" => Some((
            ApiResource {
                group: "apps".into(),
                version: "v1".into(),
                api_version: "apps/v1".into(),
                kind: "StatefulSet".into(),
                plural: "statefulsets".into(),
            },
            Scope::Namespaced,
        )),
        "svc" | "service" | "services" => Some((
            ApiResource {
                group: "".into(),
                version: "v1".into(),
                api_version: "v1".into(),
                kind: "Service".into(),
                plural: "services".into(),
            },
            Scope::Namespaced,
        )),
        "secret" | "secrets" => Some((
            ApiResource {
                group: "".into(),
                version: "v1".into(),
                api_version: "v1".into(),
                kind: "Secret".into(),
                plural: "secrets".into(),
            },
            Scope::Namespaced,
        )),
        _ => None,
    }
}

fn resolve_api_resource(
    discovery: &KubeDiscovery,
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
pub struct Status {
    #[serde(rename = "containerStatuses")]
    pub container_statuses: Option<Vec<ContainerStatus>>,

    pub phase: Option<String>,

    pub replicas: Option<u16>,

    // Node conditions
    pub conditions: Option<Vec<Condition>>,

    #[serde(rename = "readyReplicas")]
    pub ready_replicas: Option<u16>,
}

#[derive(Clone, Debug, Deserialize, Default)]
pub struct Condition {
    #[serde(rename = "type")]
    pub type_: String,

    pub status: String,
}

impl Status {
    pub fn get_ready(&self) -> String {
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

    pub fn get_status(&self) -> String {
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
