use std::fmt::Display;

use k8s_openapi::{
    api::core::v1::ContainerStatus, apimachinery::pkg::apis::meta::v1::Time, chrono::Utc,
};
use kube::{core::DynamicObject, ResourceExt};
use serde::Deserialize;
use tabled::{Style, Table, Tabled};

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
    Default_(#[tabled(inline)] DefaultOutput),
}

#[derive(Tabled, Clone, Debug)]
#[tabled(rename_all = "UPPERCASE")]
pub struct NodeOutput {
    pub clustername: String,
    pub name: String,
    pub phase: String,
    pub status: String,
    pub age: String,
}

impl From<DynamicObject> for NodeOutput {
    fn from(d: DynamicObject) -> Self {
        if let Some(status) = d.data.get("status") {
            let status: Status = serde_json::from_value(status.to_owned()).unwrap_or_default();
            Self {
                clustername: "".into(),
                name: d.name_any(),
                phase: status.phase.clone().unwrap_or_default(),
                status: status.get_status(),
                age: get_age(d.metadata.creation_timestamp),
            }
        } else {
            Self {
                clustername: "".into(),
                name: d.name_any(),
                phase: "unknown".into(),
                status: "unknown".into(),
                age: get_age(d.metadata.creation_timestamp),
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

pub fn convert_list_response_to_table(lr: ListResponse) -> Vec<KubeOutput> {
    let mut kube_output = Vec::new();
    for obj in &lr.object_list {
        match lr.kind.as_str() {
            "Node" => {
                let mut node_output: NodeOutput = obj.clone().into();
                node_output.clustername = lr.clustername.clone();
                kube_output.push(KubeOutput::Node(node_output))
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
