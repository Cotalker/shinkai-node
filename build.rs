// build.rs
use std::fs::{self, Permissions};
use std::fs::{create_dir_all, File};
use std::io::copy;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use std::env;
use std::process::Command;

fn main() {
    prost_build::compile_protos(&["protos/shinkai_message_proto.proto"], &["protos"]).unwrap();

    // Clone repo, build, and copy the Bert.cpp compiled binary server to root
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let status = Command::new("sh")
        .current_dir(&manifest_dir)
        .arg("scripts/compile_bert_cpp.sh")
        .status()
        .unwrap();
    set_execute_permission("server").expect("Failed to set execute permission");

    // Local Embedding Generator model
    let model_url = "https://huggingface.co/rustformers/pythia-ggml/resolve/main/pythia-160m-q4_0.bin";
    let model_filename = "models/pythia-160m-q4_0.bin";
    download_file(model_url, model_filename, model_filename);

    // Remote Embedding Generator model (used via LocalAI)
    let model_url = "https://huggingface.co/skeskinen/ggml/resolve/main/all-MiniLM-L12-v2/ggml-model-q4_1.bin";
    let model_filename = "models/all-MiniLM-L12-v2.bin";
    download_file(model_url, model_filename, model_filename);
}

fn set_execute_permission(path: &str) -> std::io::Result<()> {
    let permissions = Permissions::from_mode(0o755); // rwxr-xr-x
    fs::set_permissions(path, permissions)
}

fn download_file(url: &str, filename: &str, output_filename: &str) {
    // Check if the file exists
    if !Path::new(output_filename).exists() {
        // File does not exist, download it
        println!("Downloading {}...", filename);

        let response = reqwest::blocking::get(url);
        match response {
            Ok(mut resp) => {
                if resp.status().is_success() {
                    // Ensure the parent directory exists
                    if let Some(parent) = Path::new(output_filename).parent() {
                        create_dir_all(parent).expect("Failed to create directory");
                    }

                    let mut out = File::create(output_filename).expect("Failed to create file");
                    copy(&mut resp, &mut out).expect("Failed to copy content");
                    println!("{} downloaded successfully.", filename);
                } else {
                    println!("Failed to download {}: {}", filename, resp.status());
                }
            }
            Err(e) => {
                println!("Failed to download {}: {}", filename, e);
            }
        }
    } else {
        println!("{} already exists.", filename);
    }
}
