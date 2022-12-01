use anyhow::Result;
use clap::{Parser, ValueEnum};

#[derive(Debug, Parser)]
#[clap(version, about, long_about = None)]
pub struct Cli {
    pub action: Action,
    pub resource: String,
}

#[derive(Clone, PartialEq, Eq, Debug, ValueEnum)]
pub enum Action {
    Get,
}

impl Cli {
    pub async fn get(&self) -> Result<()> {
        println!("Get called!");
        Ok(())
    }
}
