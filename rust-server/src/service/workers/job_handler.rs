use crate::utils::telemetry;

pub fn begin_job(queue: &'static str, job_name: &str) {
    telemetry::record_job_started(queue, job_name);
    telemetry::record_queue_active_delta(queue, 1);
}

pub fn end_job(queue: &'static str, job_name: &str, success: bool) {
    telemetry::record_queue_active_delta(queue, -1);
    telemetry::record_job_finished(queue, job_name, success);
    telemetry::record_job_status(job_name, if success { "success" } else { "failed" });
}

pub fn end_job_with_status(queue: &'static str, job_name: &str, status: &str) {
    telemetry::record_queue_active_delta(queue, -1);
    telemetry::record_job_finished(queue, job_name, status == "success" || status == "skipped");
    telemetry::record_job_status(job_name, status);
}

pub fn worker_error(message: impl Into<String>) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(std::io::Error::new(std::io::ErrorKind::Other, message.into()))
}

pub async fn wrap_status_job<F, Fut>(
    queue: &'static str,
    job_name: &str,
    f: F,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<&'static str, String>>,
{
    begin_job(queue, job_name);
    match f().await {
        Ok("failed") => {
            end_job_with_status(queue, job_name, "failed");
            Err(worker_error("failed"))
        }
        Ok(status) => {
            end_job_with_status(queue, job_name, status);
            Ok(())
        }
        Err(err) => {
            end_job_with_status(queue, job_name, "failed");
            Err(worker_error(err))
        }
    }
}

pub async fn wrap_simple_job<F, Fut>(
    queue: &'static str,
    job_name: &str,
    f: F,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    begin_job(queue, job_name);
    match f().await {
        Ok(()) => {
            end_job_with_status(queue, job_name, "success");
            Ok(())
        }
        Err(err) => {
            end_job_with_status(queue, job_name, "failed");
            Err(worker_error(err))
        }
    }
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
