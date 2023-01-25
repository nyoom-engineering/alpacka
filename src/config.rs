use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub packages: HashMap<String, Package>,
}

#[derive(Debug, Deserialize)]
pub struct Package {
    pub rename: Option<String>,
    pub host: Option<String>,
}

impl Package {
    pub fn get_package_dirname(&self, remote_path: &str) -> String {
        let remote_path_name = remote_path.split('/').last().unwrap().to_string();
        self.rename.clone().unwrap_or(remote_path_name)
    }
}

pub fn read<P: AsRef<Path>>(path: P) -> Result<Config> {
    let cfg_bytes = std::fs::read(path)?;
    Ok(serde_yaml::from_slice(&cfg_bytes)?)
}
