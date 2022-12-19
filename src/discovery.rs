use anyhow::{anyhow, Result};
use async_recursion::async_recursion;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tracing::log::debug;

use kube::discovery::{ApiResource, Scope};

pub struct Discovery {
    resources: Vec<DiscoveryResource>,
}

pub struct DiscoveryResource {
    /// Matches for ApiResource. Used to match kind, shortname, and plural (po, pod, pods for kind
    /// Pod.
    kind: Vec<String>,
    /// Resource provided to Api client to get resource
    api_resource: ApiResource,
    /// Whether this resource is namespace or cluster scoped
    scope: Scope,
}

impl Discovery {
    /// Creates a
    pub async fn new_from_default_cache(url: String) -> Result<Self> {
        let mut resources = Vec::new();
        let host_path = parse_kube_url_to_discovery(url)?;
        let paths = get_cache_files(
            dirs::home_dir()
                .unwrap()
                .join(".kube")
                .join("cache")
                .join("discovery")
                .join(host_path),
        )?;
        let files = read_cache_files(paths);
        for file in &files {
            match ApiResourceList::try_from_str(file) {
                Ok(arl) => resources.append(&mut arl.get_api_resources()),
                Err(e) => debug!("failed to parse discovery {}", e),
            }
        }
        let discovery = Discovery { resources };
        Ok(discovery)
    }

    pub fn get_resource_from_name(&self, name: &str) -> Result<(ApiResource, Scope)> {
        for resource in &self.resources {
            for k in &resource.kind {
                if k.eq_ignore_ascii_case(name) {
                    return Ok((resource.api_resource.clone(), resource.scope.clone()));
                }
            }
        }
        Err(anyhow!("resource {} not found", name))
    }
}

// Replacement taken from: https://github.com/kubernetes/kubernetes/blob/c4d752765b3bbac2237bf87cf0b1c2e307844666/staging/src/k8s.io/cli-runtime/pkg/genericclioptions/config_flags.go#L355-L365
pub fn parse_kube_url_to_discovery(url: String) -> Result<String> {
    let re = regex::Regex::new(r"[^(\w/\.)]").unwrap();
    let hp = url
        .replace("https://", "")
        .replace("http://", "")
        .replace('/', "");
    Ok(re.replace_all(&hp, "_").to_string())
}

//#[async_recursion]
//async fn get_cache_files<P: AsRef<Path> + Send>(path: P) -> Result<Vec<PathBuf>> {
//    let mut files: Vec<PathBuf> = Vec::new();
//    let mut entries = tokio::fs::read_dir(path).await?;
//    while let Some(entry) = entries.next_entry().await? {
//        if let Ok(file_type) = entry.file_type().await {
//            if file_type.is_dir() {
//                let mut recurse_entries = get_cache_files(entry.path()).await?;
//                files.append(&mut recurse_entries)
//            } else if file_type.is_file() && is_json(&entry.path()) {
//                files.push(entry.path())
//            }
//        }
//    }
//    Ok(files)
//}
//
//async fn read_cache_files(paths: Vec<PathBuf>) -> Vec<String> {
//    let mut file_outs: Vec<String> = Vec::new();
//    for path in paths {
//        if let Ok(file_out) = tokio::fs::read_to_string(path).await {
//            file_outs.push(file_out);
//        }
//    }
//    file_outs
//}
fn get_cache_files<P: AsRef<Path> + Send>(path: P) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = Vec::new();
    let mut entries = std::fs::read_dir(path)?;
    while let Some(Ok(entry)) = entries.next() {
        if let Ok(file_type) = entry.file_type() {
            if file_type.is_dir() {
                let mut recurse_entries = get_cache_files(entry.path())?;
                files.append(&mut recurse_entries)
            } else if file_type.is_file() && is_json(&entry.path()) {
                files.push(entry.path())
            }
        }
    }
    Ok(files)
}

fn read_cache_files(paths: Vec<PathBuf>) -> Vec<String> {
    let mut file_outs: Vec<String> = Vec::new();
    for path in paths {
        if let Ok(file_out) = std::fs::read_to_string(path) {
            file_outs.push(file_out);
        }
    }
    file_outs
}

fn is_json(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        ext == "json"
    } else {
        false
    }
}

#[allow(unused)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiResourceList {
    kind: String,
    api_version: String,
    group_version: String,
    resources: Vec<Resource>,
}

impl ApiResourceList {
    fn try_from_str(input: &str) -> Result<Self> {
        match serde_json::from_str(input) {
            Ok(arl) => Ok(arl),
            Err(e) => Err(anyhow!("failed to parse ApiResourceList: {}", e)),
        }
    }
    fn get_api_resources(&self) -> Vec<DiscoveryResource> {
        let group = match self.group_version.split_once('/') {
            Some(g) => g.0,
            None => "",
        };
        let mut resource_list = Vec::new();
        for resource in &self.resources {
            let api_resource = ApiResource {
                group: group.to_string(),
                version: self.api_version.clone(),
                api_version: self.group_version.clone(),
                kind: resource.kind.clone(),
                plural: resource.name.clone(),
            };
            let scope = if resource.namespaced {
                Scope::Namespaced
            } else {
                Scope::Cluster
            };

            let mut kind = vec![
                resource.kind.clone().to_lowercase(),
                resource.name.clone().to_lowercase(),
            ];
            if let Some(short) = &resource.short_names {
                for s in short {
                    kind.push(s.to_string());
                }
            }
            resource_list.push(DiscoveryResource {
                kind,
                api_resource,
                scope,
            });
        }
        resource_list
    }
}

#[allow(unused)]
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Resource {
    name: String,
    singular_name: Option<String>,
    namespaced: bool,
    kind: String,
    group: Option<String>,
    short_names: Option<Vec<String>>,
    verbs: Vec<Verb>,
    storage_version_hash: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Verb {
    Create,
    Delete,
    DeleteCollection,
    Get,
    List,
    Patch,
    Update,
    Watch,
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn walk_dirs() {
        let _files = get_cache_files(
            "/home/randerson/.kube/cache/discovery/carson.cloud.gravitational.io_443/",
        )
        .unwrap();
    }

    //#[test]
    //fn build_single_resource_map() {
    //    let arl = ApiResourceList {
    //        kind: "APIResourceList".into(),
    //        api_version: "v1".into(),
    //        group_version: "apps/v1".into(),
    //        resources: vec![Resource {
    //            name: "daemonsets".into(),
    //            singular_name: "".into(),
    //            namespaced: true,
    //            kind: "DaemonSet".into(),
    //            short_names: Some(vec!["ds".into()]),
    //            verbs: vec![Verb::Get],
    //            storage_version_hash: "".into(),
    //        }],
    //    };
    //    let now = tokio::time::Instant::now();
    //    let list = arl.get_api_resources();
    //    //let map = create_map_from_arls(list);
    //    assert_eq!(map.get("ds").unwrap().0.kind, "DaemonSet".to_string());
    //    println!("time to build and fetch from map {:?}", now.elapsed());
    //}
    #[tokio::test]
    async fn build_all_resource_map() {
        let now = tokio::time::Instant::now();
        let dis =
            Discovery::new_from_default_cache("https://carson.cloud.gravitational.io:443".into())
                .await
                .unwrap();
        let ds = dis.get_resource_from_name("DaemonSet").unwrap();
        println!("time taken to parse and find resource {:?}", now.elapsed());
        assert_eq!(ds.0.kind, "DaemonSet");
    }

    #[test]
    fn build_host_path() {
        let hp = parse_kube_url_to_discovery("https://carson.cloud.gravitational.io:443".into())
            .unwrap();
        assert_eq!(hp, "carson.cloud.gravitational.io_443");
    }
}
