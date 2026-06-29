use crate::utils::telemetry;

pub fn begin_job(queue: &'static str, job_name: &str) {
    telemetry::record_job_started(queue, job_name);
}

pub fn end_job(queue: &'static str, job_name: &str, success: bool) {
    telemetry::record_job_finished(queue, job_name, success);
}

pub fn worker_error(message: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(std::io::Error::new(std::io::ErrorKind::Other, message.into()))
}

pub fn finish_failed(
    queue: &'static str,
    job_name: &str,
    message: impl Into<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    begin_job(queue, job_name);
    end_job(queue, job_name, false);
    Err(worker_error(message))
}

pub fn finish_ok(queue: &'static str, job_name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    begin_job(queue, job_name);
    end_job(queue, job_name, true);
    Ok(())
}
