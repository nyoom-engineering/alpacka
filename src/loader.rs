use anyhow::Result;
use bincode2::{deserialize, deserialize_from, serialize};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
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

pub fn read<P: AsRef<Path> + Copy>(path: &P) -> Result<(usize, Config)> {
    let json_bytes = fs::read(path)?;
    let json_hash = hash_json(&json_bytes)?;
    let index_path = Path::new("index.bin");
    let bincode_path = format!("{}.bin", json_hash);
    let (generation, config) = if let Ok(bincode_metadata) = fs::metadata(&bincode_path) {
        let index_data: HashMap<String, usize> = deserialize_from(fs::File::open(index_path)?)?;
        if let Some(gen) = index_data.get(&json_hash) {
            println!(
                "Loading bincode for generation {} for hash {}",
                gen, json_hash
            );
            let config = deserialize(&fs::read(&bincode_path)?)?;
            (*gen, config)
        } else {
            let new_gen = index_data.len();
            println!(
                "Creating new bincode for generation {} for hash {}",
                new_gen, json_hash
            );
            let config = serde_json::from_slice(&json_bytes)?;
            write_to_index(index_path, json_hash.clone(), new_gen)?;
            serialize_to_file(&config, &bincode_path)?;
            (new_gen, config)
        }
    } else {
        println!(
            "Creating new bincode for generation 0 for hash {}",
            json_hash
        );
        let index_data: HashMap<String, usize> = HashMap::new();
        let config = serde_json::from_slice(&json_bytes)?;
        write_to_index(index_path, json_hash.clone(), 0)?;
        serialize_to_file(&config, &bincode_path)?;
        (0, config)
    };
    Ok((generation, config))
}

fn hash_json(json: &[u8]) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(json);

    Ok(format!("{:x}", hasher.finalize()))
}

fn write_to_index<P: AsRef<Path> + Copy>(
    index_path: P,
    hash: String,
    generation: usize,
) -> Result<()> {
    let index_data = match fs::metadata(index_path) {
        Ok(_) => deserialize_from(fs::File::open(index_path)?)?,
        Err(_) => HashMap::new(),
    };
    let mut index_data = index_data;
    index_data.insert(hash, generation);
    let encoded = serialize(&index_data)?;
    fs::write(index_path, encoded)?;
    Ok(())
}

fn serialize_to_file<P: AsRef<Path>>(config: &Config, file_path: P) -> Result<()> {
    let encoded = serialize(config)?;
    fs::write(file_path, encoded)?;
    Ok(())
}
