use anyhow::{Context, Result};
use futures::StreamExt;
use k8s_openapi::api::core::v1::ContainerStatus;
use kube::{
    api::ListParams,
    config::Kubeconfig,
    core::{DynamicObject, ObjectList},
    discovery::{ApiCapabilities, ApiResource, Scope},
    Api, Client as KubeClient, Discovery,
};
use serde::Deserialize;
use std::fmt::Display;
use tracing::log::{debug, warn};

use crate::config::Cluster;

pub struct Client {
    kubeclients: Vec<(String, Api<DynamicObject>)>,
}

pub struct ListResponse {
    pub clustername: String,
    pub object_list: ObjectList<DynamicObject>,
}

//#[async_trait]
//pub trait MCClient {
//    async fn get(&self, name: String) -> Result<()>;
//    async fn list(&self) -> Result<Vec<Output>>;
//}

impl Client {
    pub async fn try_new(clusters: &Vec<Cluster>, namespace: &str, resource: &str) -> Result<Self> {
        let mut kubeclients: Vec<(String, Api<DynamicObject>)> = Vec::new();
        for cluster in clusters {
            let clustername = cluster.name.clone();
            let kubeconfig = Kubeconfig::read()?;
            let options = cluster.into();

            let config = kube::config::Config::from_custom_kubeconfig(kubeconfig, &options).await?;
            let client = KubeClient::try_from(config)?;

            // Check common/known resources before using discovery
            if let Some(r) = known_resources(resource) {
                let client = create_client(client, r.0, r.1, namespace);
                kubeclients.push((clustername, client));
                continue;
            }

            let discovery = Discovery::new(client.clone())
                .run()
                .await
                .context("failed to discover api resources")?;

            let ar_cap = resolve_api_resource(&discovery, resource);

            if let Some((ar, cap)) = ar_cap {
                let client = create_client(client, ar, cap.scope, namespace);
                kubeclients.push((clustername, client));
            } else {
                debug!("failed to create client for cluster {}", clustername)
            }
        }
        Ok(Client { kubeclients })
    }

    pub async fn list(self) -> Result<Vec<ListResponse>> {
        Ok(list_resources(self, &ListParams::default()).await)
    }
}

// Fetch resources using all clients in parallel
async fn list_resources(client: Client, lp: &ListParams) -> Vec<ListResponse> {
    let req_count = client.kubeclients.len();
    let bodies = futures::stream::iter(client.kubeclients).map(|client| {
        let lp = lp.clone();
        tokio::spawn(async move {
            let response = client.1.list(&lp).await;
            (client.0, response)
        })
    }).buffer_unordered(req_count);
    //let mut lrs: Vec<ListResponse> = Vec::new();
    let lrs = bodies.for_each(|b| async {

        let mut lrs: Vec<ListResponse> = Vec::new();
        match b {
            Ok((cn, Ok(resp))) => {
                lrs.push(ListResponse{
                    clustername: cn,
                    object_list: resp,
                })
            },
            Ok((_, Err(_e))) => {},
            Err(_) => todo!(),
        }
        lrs
    }).await;
    //let handles = futures::future::join_all(client.kubeclients.into_iter().map(|client| {
    //    let lp = lp.clone();
    //    tokio::spawn(async move {
    //        let response = client.1.list(&lp).await;
    //        (client.0, response)
    //    })
    //}))
    //.await;

    //let mut lr: Vec<ListResponse> = Vec::new();
    //for handle in handles {
    //    match handle {
    //        Ok(h) => {
    //            if let Ok(object_list) = h.1 {
    //                lr.push(ListResponse {
    //                    clustername: h.0,
    //                    object_list,
    //                })
    //            } else {
    //                warn!("failed request to cluster {}", h.0)
    //            }
    //        }
    //        Err(e) => {
    //            debug!("join handle failed {}", e)
    //        }
    //    }
    //}
    lrs
}

fn create_client(
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
