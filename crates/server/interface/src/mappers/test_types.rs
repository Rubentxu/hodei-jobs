#[cfg(test)]
mod grpc_types_test {
    use hodei_jobs::job::{JobTemplate, ResourceRequirements, TemplateExecution};

    #[test]
    fn test_resource_requirements_fields() {
        let rr = ResourceRequirements {
            cpu_cores: 1.0,
            memory_mb: 1024,
            storage_mb: 2048,
            gpu_required: false,
            architecture: "x86_64".to_string(),
        };

        // Verify fields exist
        assert_eq!(rr.cpu_cores, 1.0);
        assert_eq!(rr.memory_mb, 1024);
    }

    #[test]
    fn test_job_template_exists() {
        let _jt = JobTemplate {
            template_id: "test".to_string(),
            name: "test".to_string(),
            description: "test".to_string(),
            spec: Default::default(),
            status: "Active".to_string(),
            version: 1,
            labels: Default::default(),
            created_at: None,
            updated_at: None,
            created_by: "test".to_string(),
            run_count: 0,
            success_count: 0,
            failure_count: 0,
        };
    }
}
