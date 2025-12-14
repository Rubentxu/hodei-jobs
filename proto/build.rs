use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=hodei_all_in_one.proto");
    println!("cargo:rerun-if-changed=provider_management.proto");
    
    let out_dir = PathBuf::from("src/generated");
    let descriptor_path = out_dir.join("hodei_descriptor.bin");
    
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(&out_dir)
        .file_descriptor_set_path(&descriptor_path)
        .compile_protos(
            &["hodei_all_in_one.proto", "provider_management.proto"],
            &["."]
        )
        .expect("Failed to compile proto files");
}