use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=hodei_all_in_one.proto");
    println!("cargo:rerun-if-changed=provider_management.proto");
    println!("cargo:rerun-if-changed=worker_agent.proto");
    println!("cargo:rerun-if-changed=job_templates.proto");
    println!("cargo:rerun-if-changed=scheduled_jobs.proto");
    println!("cargo:rerun-if-changed=common.proto");

    let out_dir = PathBuf::from("src/generated");
    let descriptor_path = out_dir.join("hodei_descriptor.bin");

    // Create output directory if it doesn't exist
    std::fs::create_dir_all(&out_dir)?;

    // Use prost_build::Config to pass the experimental flag
    let mut config = prost_build::Config::new();
    config.protoc_arg("--experimental_allow_proto3_optional");

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(&out_dir)
        .file_descriptor_set_path(&descriptor_path)
        .compile_with_config(
            config,
            &[
                "hodei_all_in_one.proto",
                "provider_management.proto",
                "worker_agent.proto",
                "job_templates.proto",
                "scheduled_jobs.proto",
            ],
            &["."],
        )?;

    Ok(())
}
