use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=hodei_all_in_one.proto");
    println!("cargo:rerun-if-changed=hodei.v1.proto");
    println!("cargo:rerun-if-changed=provider_management.proto");
    println!("cargo:rerun-if-changed=worker_agent.proto");
    println!("cargo:rerun-if-changed=job_templates.proto");
    println!("cargo:rerun-if-changed=scheduled_jobs.proto");

    let out_dir = PathBuf::from("src/generated");
    let descriptor_path = out_dir.join("hodei_descriptor.bin");

    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(&out_dir)
        .file_descriptor_set_path(&descriptor_path)
        .compile_protos(
            &[
                "hodei.v1.proto",         // NEW: Unified package
                "hodei_all_in_one.proto", // Keep for backward compatibility during transition
                "provider_management.proto",
                "worker_agent.proto",
                "job_templates.proto",
                "scheduled_jobs.proto",
            ],
            &["."],
        )
        .expect("Failed to compile proto files");
}
