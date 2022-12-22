use anyhow::{anyhow, Context, Result};
use kube::{
    api::ListParams,
    config::{KubeConfigOptions, Kubeconfig},
    core::{DynamicObject, ObjectList},
    discovery::{ApiCapabilities, ApiResource, Scope},
    Api, Client as KubeClient, Discovery as KubeDiscovery,
};
use std::sync::Arc;
use tracing::log::{debug, warn};

use crate::{config::Cluster, discovery::Discovery};

type ClusterName = String;
type Kind = String;
type MCCluster = (ClusterName, Api<DynamicObject>, Kind);

pub struct Client {
    pub kind: String,
    kubeclients: Vec<MCCluster>,
}

pub struct ListResponse {
    pub clustername: String,
    pub kind: String,
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
        let mut kind = String::new();
        let mut kubeclients: Vec<MCCluster> = Vec::new();
        for handle in handles {
            match handle {
                Ok(Ok(mcclient)) => {
                    kind = mcclient.2.clone();
                    kubeclients.push(mcclient)
                }
                Ok(Err(e)) => warn!("failed to create client {}", e),
                Err(e) => debug!("join failed {}", e),
            }
        }
        Ok(Client { kind, kubeclients })
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
            let kind = resource.kind.clone();
            let client = create_typed_kubeclient(client, resource, scope, namespace);
            return Ok((clustername, client, kind));
        }
    }

    let kube_discovery = KubeDiscovery::new(client.clone())
        .run()
        .await
        .context("failed to discover api resources")?;

    let ar_cap = resolve_api_resource(&kube_discovery, resource);

    if let Some((ar, cap)) = ar_cap {
        let kind = ar.kind.clone();
        let client = create_typed_kubeclient(client, ar, cap.scope, namespace);
        Ok((clustername, client, kind))
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
    let kind = client.kind;
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
                        kind: kind.clone(),
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
