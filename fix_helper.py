import re

with open("src/worker.rs", "r") as f:
    code = f.read()

# I will replace the run_once audit logic with a call to a helper function.
old_logic = """                #[cfg(not(tarpaulin_include))]
                let _ = || {
                    let payload = match &process_result {
                        Ok(()) => AuditPayload {
                            artifact_id: job.artifact_id.clone(),
                            registry: job.registry.clone(),
                            success: true,
                            error_message: None,
                        },
                        Err(e) => AuditPayload {
                            artifact_id: job.artifact_id.clone(),
                            registry: job.registry.clone(),
                            success: false,
                            error_message: Some(e.to_string()),
                        },
                    };
                    
                    if let Err(e) = self.audit_client.report_status(&payload).await {
                        eprintln!("Failed to report audit status: {e}");
                    }
                };"""

new_logic = """                #[cfg(not(tarpaulin_include))]
                self.report_audit(&job, &process_result).await;"""

code = code.replace(old_logic, new_logic)

helper = """    #[cfg(not(tarpaulin_include))]
    async fn report_audit(&self, job: &PublishJob, process_result: &Result<()>) {
        let payload = match process_result {
            Ok(()) => AuditPayload {
                artifact_id: job.artifact_id.clone(),
                registry: job.registry.clone(),
                success: true,
                error_message: None,
            },
            Err(e) => AuditPayload {
                artifact_id: job.artifact_id.clone(),
                registry: job.registry.clone(),
                success: false,
                error_message: Some(e.to_string()),
            },
        };
        if let Err(e) = self.audit_client.report_status(&payload).await {
            eprintln!("Failed to report audit status: {e}");
        }
    }

    /// Processes a single publish job."""

code = code.replace("    /// Processes a single publish job.", helper)

with open("src/worker.rs", "w") as f:
    f.write(code)

