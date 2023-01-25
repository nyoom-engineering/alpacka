use anyhow::Result;
use bincode2::{deserialize, serialize};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub packages: HashMap<String, Package>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Package {
    pub opt: Option<bool>,
    pub ver: Option<String>,
    pub rename: Option<String>,
    pub branch: Option<String>,
    pub commit: Option<String>,
    pub build: Option<String>,
    // pub dependencies: Vec<Package>,
}

impl Package {
    pub fn get_package_dirname(&self, remote_path: &str) -> String {
        let remote_path_name = remote_path.split('/').last().unwrap().to_string();
        self.rename.clone().unwrap_or(remote_path_name)
    }
}

pub fn read<P: AsRef<Path>>(path: &P) -> Result<Config> {
    let cfg_bytes = std::fs::read(path)?;
    let config = serde_json::from_slice(&cfg_bytes)?;
    let hash = hash_file(path)?;
    let filename = format!("{}.bin", hash);
    let file_path = Path::new(&filename);
    serialize_to_file(&config, &file_path)?;
    Ok(config)
}

fn hash_file<P: AsRef<Path>>(path: P) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn serialize_to_file<P: AsRef<Path>>(config: &Config, file_path: P) -> Result<()> {
    let encoded: Vec<u8> = serialize(config)?;
    std::fs::write(file_path, encoded)?;
    Ok(())
}

// create "generation" using bincode2
// store generation index?
