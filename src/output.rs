use std::fmt::Display;

use k8s_openapi::{
    api::{
        apps::v1::DeploymentStatus,
        core::v1::{ContainerStatus, NodeStatus, PodSpec, PodStatus, ServiceSpec, ServiceStatus},
    },
    apimachinery::pkg::apis::meta::v1::Time,
    chrono::Utc,
};
use kube::{core::DynamicObject, ResourceExt};
use serde::Deserialize;
use tabled::{settings::Style, Table, Tabled};

use crate::client::ListResponse;

#[derive(Tabled, Clone, Debug)]
#[tabled(rename_all = "UPPERCASE")]
pub struct Output {
    pub cluster: String,
    pub namespace: String,
    pub name: String,
    pub status: String,
    pub ready: String,
    pub age: String,
}

#[derive(Tabled, Clone, Debug)]
#[tabled(rename_all = "UPPERCASE")]
pub enum KubeOutput {
    #[tabled(inline)]
    Node(#[tabled(inline)] NodeOutput),
    #[tabled(inline)]
    Pod(#[tabled(inline)] PodOutput),
    #[tabled(inline)]
    Deployment(#[tabled(inline)] DeploymentOutput),
    #[tabled(inline)]
    Service(#[tabled(inline)] ServiceOutput),
    #[tabled(inline)]
    Default_(#[tabled(inline)] DefaultOutput),
}

#[derive(Tabled, Clone, Debug, Default)]
#[tabled(rename_all = "UPPERCASE")]
pub struct NodeOutput {
    pub clustername: String,
    pub name: String,
    pub status: String,
    pub age: String,
    pub version: String,
    pub arch: String,
    pub kernel: String,
    pub container_runtime_version: String,
}

impl From<DynamicObject> for NodeOutput {
    fn from(d: DynamicObject) -> Self {
        if let Some(status) = d.data.get("status") {
            let status: NodeStatus = serde_json::from_value(status.to_owned()).unwrap_or_default();
            let node_info = status.node_info.clone().unwrap_or_default();
            let conditions = status.conditions.unwrap_or_default();
            Self {
                clustername: "".into(),
                name: d.name_any(),
                status: conditions
                    .iter()
                    .find(|condition| condition.type_ == "Ready")
                    .map_or_else(
                        || "Unknown".to_string(),
                        |condition| {
                            if condition.status == "True" {
                                "Ready".to_string()
                            } else {
                                "NotReady".to_string()
                            }
                        },
                    ),
                age: get_age(d.metadata.creation_timestamp),
                version: node_info.kubelet_version,
                arch: node_info.architecture,
                kernel: node_info.kernel_version,
                container_runtime_version: node_info.container_runtime_version,
            }
        } else {
            Self {
                clustername: "".into(),
                name: d.name_any(),
                status: "Unknown".into(),
                age: get_age(d.metadata.creation_timestamp),
                ..Default::default()
            }
        }
    }
}

#[derive(Tabled, Clone, Debug)]
#[tabled(rename_all = "UPPERCASE")]
pub struct DefaultOutput {
    pub clustername: String,
    pub name: String,
    pub age: String,
}

impl From<DynamicObject> for DefaultOutput {
    fn from(d: DynamicObject) -> Self {
        Self {
            clustername: "".into(),
            name: d.name_any(),
            age: get_age(d.metadata.creation_timestamp),
        }
    }
}

#[derive(Tabled, Clone, Debug, Default)]
#[tabled(rename_all = "UPPERCASE")]
pub struct PodOutput {
    pub clustername: String,
    pub name: String,
    pub status: String,
    pub restarts: String,
    pub age: String,
    pub ip: String,
    pub node: String,
}

impl From<DynamicObject> for PodOutput {
    fn from(d: DynamicObject) -> Self {
        if let (Some(status), Some(spec)) = (d.data.get("status"), d.data.get("spec")) {
            let spec: PodSpec = serde_json::from_value(spec.to_owned()).unwrap_or_default();
            let status: PodStatus = serde_json::from_value(status.to_owned()).unwrap_or_default();
            let container_statuses = status.container_statuses.unwrap_or_default();
            let init_containers = status.init_container_statuses.unwrap_or_default();
            Self {
                clustername: "".into(),
                name: d.name_any(),
                status: status.phase.unwrap_or_else(|| "Unknown".to_string()),
                restarts: {
                    let mut restart_count = 0;
                    container_statuses
                        .iter()
                        .for_each(|cs| restart_count += cs.restart_count);
                    init_containers
                        .iter()
                        .for_each(|cs| restart_count += cs.restart_count);
                    restart_count.to_string()
                },
                age: get_age(d.metadata.creation_timestamp),
                ip: status.pod_ip.unwrap_or_default(),
                node: spec.node_name.unwrap_or_default(),
            }
        } else {
            Self {
                clustername: "".into(),
                name: d.name_any(),
                status: "Unknown".into(),
                age: get_age(d.metadata.creation_timestamp),
                ..Default::default()
            }
        }
    }
}

#[derive(Tabled, Clone, Debug, Default)]
#[tabled(rename_all = "UPPERCASE")]
pub struct DeploymentOutput {
    pub clustername: String,
    pub name: String,
    pub ready: String,
    pub up_to_date: String,
    pub available: String,
    pub age: String,
}

impl From<DynamicObject> for DeploymentOutput {
    fn from(d: DynamicObject) -> Self {
        if let (Some(status), Some(spec)) = (d.data.get("status"), d.data.get("spec")) {
            let status: DeploymentStatus =
                serde_json::from_value(status.to_owned()).unwrap_or_default();
            Self {
                clustername: "".into(),
                name: d.name_any(),
                ready: format!(
                    "{}/{}",
                    status.ready_replicas.unwrap_or_default(),
                    status.replicas.unwrap_or_default(),
                ),
                up_to_date: status.updated_replicas.unwrap_or_default().to_string(),
                available: status.available_replicas.unwrap_or_default().to_string(),
                age: get_age(d.metadata.creation_timestamp),
            }
        } else {
            Self {
                clustername: "".into(),
                name: d.name_any(),
                age: get_age(d.metadata.creation_timestamp),
                ..Default::default()
            }
        }
    }
}

#[derive(Tabled, Clone, Debug, Default)]
#[tabled(rename_all = "UPPERCASE")]
pub struct ServiceOutput {
    pub clustername: String,
    pub name: String,
    pub type_: String,
    pub cluster_ip: String,
    pub external_ip: String,
    pub ports: String,
    pub age: String,
    pub selector: String,
}

impl From<DynamicObject> for ServiceOutput {
    fn from(d: DynamicObject) -> Self {
        if let (Some(status), Some(spec)) = (d.data.get("status"), d.data.get("spec")) {
            let spec: ServiceSpec = serde_json::from_value(spec.to_owned()).unwrap_or_default();
            let status: ServiceStatus =
                serde_json::from_value(status.to_owned()).unwrap_or_default();
            Self {
                clustername: "".into(),
                name: d.name_any(),
                type_: spec.type_.unwrap_or("Unknown".to_string()),
                cluster_ip: spec.cluster_ip.unwrap_or("<none>".to_string()),
                external_ip: get_external_ip(&status),
                ports: spec
                    .ports
                    .unwrap_or_default()
                    .iter()
                    .map(|port| {
                        format!(
                            "{}/{}",
                            port.port,
                            port.protocol.as_deref().unwrap_or_default()
                        )
                    })
                    .collect::<Vec<String>>()
                    .join(","),
                age: get_age(d.metadata.creation_timestamp),
                selector: spec
                    .selector
                    .unwrap_or_default()
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect::<Vec<String>>()
                    .join(","),
            }
        } else {
            Self {
                clustername: "".into(),
                name: d.name_any(),
                age: get_age(d.metadata.creation_timestamp),
                ..Default::default()
            }
        }
    }
}

pub fn convert_list_response_to_table(lr: ListResponse) -> Vec<KubeOutput> {
    let mut kube_output = Vec::new();
    for obj in &lr.object_list {
        match lr.kind.as_str() {
            "Node" => {
                let mut output: NodeOutput = obj.clone().into();
                output.clustername = lr.clustername.clone();
                kube_output.push(KubeOutput::Node(output))
            }
            "Pod" => {
                let mut output: PodOutput = obj.clone().into();
                output.clustername = lr.clustername.clone();
                kube_output.push(KubeOutput::Pod(output))
            }
            "Deployment" => {
                let mut output: DeploymentOutput = obj.clone().into();
                output.clustername = lr.clustername.clone();
                kube_output.push(KubeOutput::Deployment(output))
            }
            "Service" => {
                let mut output: ServiceOutput = obj.clone().into();
                output.clustername = lr.clustername.clone();
                kube_output.push(KubeOutput::Service(output))
            }
            _ => {
                let mut default_output: DefaultOutput = obj.clone().into();
                default_output.clustername = lr.clustername.clone();
                kube_output.push(KubeOutput::Default_(default_output))
            }
        }
    }
    kube_output
}

pub(crate) fn create_table<T: Tabled>(outputs: Vec<T>) {
    let mut builder = Table::builder(&outputs);
    builder.clean();
    let table = builder.build().with(Style::blank()).to_string();
    println!("{}", table)
}
//pub(crate) fn create_table<T: Tabled>(outputs: Vec<T>) {
//    let mut table = Table::new(&outputs);
//    table.with(Style::blank());
//    println!("{}", table)
//}

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

fn get_external_ip(status: &ServiceStatus) -> String {
    let default = "<none>".to_string();
    let Some(lb) = &status.load_balancer else {return default};
    let Some(ing) = &lb.ingress else {return default};
    if let Some(first_ing) = ing.first() {
        if let Some(ip) = &first_ing.ip {
            return ip.to_owned();
        } else if let Some(host) = &first_ing.hostname {
            return host.to_owned();
        }
    }
    default
}
