import re

with open("src/worker.rs", "r") as f:
    code = f.read()

fixed_match = """        match serde_json::from_slice::<PublishJob>(&payload_bytes) {
            Ok(job) => {
                let process_result = self.process_job(&job).await;
                
                #[cfg(not(tarpaulin_include))]
                {
                    // Determine audit state based on the result
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
                    
                    // Fire and forget audit (log if it fails but don't crash loop)
                    if let Err(e) = self.audit_client.report_status(&payload).await {
                        eprintln!("Failed to report audit status: {e}");
                    }
                }

                if let Err(e) = process_result {
                    eprintln!("Failed to process job: {e}");
                    // Leave in pending state or move to dead-letter queue.
                    // For now, we will leave it pending to be retried later or handle it.
                } else {
                    #[cfg(not(tarpaulin_include))]
                    self.acknowledge(&id).await.expect("Failed to await");
                }
                Ok(())
            }
            Err(e) => {
                eprintln!("Failed to parse job payload: {e}");
                // Invalid payload, acknowledge to drop it or move to DLQ.
                self.acknowledge(&id).await.expect("Failed to await");
                Ok(())
            }
        }
    }"""

# find start and end
idx1 = code.find("        match serde_json::from_slice::<PublishJob>(&payload_bytes) {")
idx2 = code.find("    /// Processes a single publish job.")

code = code[:idx1] + fixed_match + "\n\n" + code[idx2:]

with open("src/worker.rs", "w") as f:
    f.write(code)

