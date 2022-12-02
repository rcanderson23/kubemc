use std::io::{self, Write};

use anyhow::Result;
use clap::{Parser, ValueEnum};

use crate::config::MCConfig;

#[derive(Debug, Parser)]
#[clap(version, about, long_about = None)]
pub struct Cli {
    /// Action for CLI to use
    pub action: Action,

    /// Kubernetes resource to apply action to
    pub resource: Option<String>,

    /// Path to config file
    pub config_file: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, ValueEnum)]
pub enum Action {
    Get,
    GenerateConfig,
}

impl Cli {
    pub async fn get(&self) -> Result<()> {
        println!("Get called!");
        Ok(())
    }

    pub async fn generate_config(&self) -> Result<()> {
        let config_yaml = MCConfig::yaml()?;
        io::stdout().write(config_yaml.as_bytes()).map(|_| Ok(()))?
    }
}
