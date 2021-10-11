use diesel::prelude::*;
use diesel::sql_types;
use diesel::{insert_into, update};
use serde_json;

use crate::errors::EnqueueError;
use crate::schema::background_jobs;
use crate::serde::__private::Formatter;
use crate::Job;

#[derive(Debug)]
pub enum JobStatus {
    Queued,
    Succeeded,
    FailedReQueued,
    Failed,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> serde::__private::fmt::Result {
        match self {
            JobStatus::Queued => write!(f, "Queued"),
            JobStatus::Succeeded => write!(f, "Succeeded"),
            JobStatus::FailedReQueued => write!(f, "FailedReQueued"),
            JobStatus::Failed => write!(f, "Failed"),
        }
    }
}

#[derive(Queryable, Selectable, Debug, Clone)]
pub struct BackgroundJob {
    pub id: String,
    pub job_type: String,
    pub data: serde_json::Value,
    pub retries: i32,
}

#[derive(Queryable, Insertable, Debug, Clone)]
#[table_name = "background_jobs"]
pub struct NewBackgroundJob {
    pub id: String,
    pub job_type: String,
    pub data: serde_json::Value,
    pub status: String,
}

/// Enqueues a job to be run as soon as possible.
pub fn enqueue_job<T: Job>(conn: &mut PgConnection, job: T) -> Result<(), EnqueueError> {
    use crate::schema::background_jobs::dsl::*;

    let job_data = serde_json::to_value(job)?;
    insert_into(background_jobs)
        .values(NewBackgroundJob {
            id: min_id::generate_id(),
            job_type: T::JOB_TYPE.to_owned(),
            data: job_data,
            status: JobStatus::Queued.to_string(),
        })
        .execute(conn)?;
    Ok(())
}

/// Finds the next job that is unlocked and ready to be retried. If a row is
/// found, it gets locked.
pub fn find_next_unlocked_job(
    conn: &mut PgConnection,
    max_retries: i32,
) -> QueryResult<BackgroundJob> {
    use crate::schema::background_jobs::dsl;
    use diesel::dsl::{now, IntervalDsl};

    sql_function!(fn power(x: sql_types::Integer, y: sql_types::Integer) -> sql_types::Integer);
    sql_function!(fn to_timestamp(x: sql_types::Integer) -> sql_types::Timestamp);
    sql_function!(fn coalesce(x: sql_types::Nullable<sql_types::Timestamp>, y: sql_types::Timestamp) -> sql_types::Timestamp);

    dsl::background_jobs
        .select(BackgroundJob::as_select())
        .filter(dsl::status.eq_any(vec![
            JobStatus::Queued.to_string(),
            JobStatus::FailedReQueued.to_string(),
        ]))
        .filter(dsl::retries.lt(max_retries))
        .filter(
            dsl::last_retry_at.is_null().or(coalesce(dsl::last_retry_at, to_timestamp(0))
                .lt(now - 1.minute().into_sql::<sql_types::Interval>() * power(2, dsl::retries))),
        )
        .order(dsl::created_at)
        .for_update()
        .skip_locked()
        .first::<BackgroundJob>(conn)
}

/// Updates the status of a job that has successfully completed running.
pub fn update_successful_job(conn: &mut PgConnection, job_id: &String) -> QueryResult<()> {
    use crate::schema::background_jobs::dsl;

    update(dsl::background_jobs.find(job_id))
        .set(dsl::status.eq(JobStatus::Succeeded.to_string()))
        .execute(conn)?;

    Ok(())
}

/// Marks that we just tried and failed to run a job.
///
/// Ignores any database errors that may have occurred. If the DB has gone away,
/// we assume that just trying again with a new connection will succeed.
pub fn update_failed_job(
    conn: &mut PgConnection,
    job_id: &String,
    job_retries: i32,
    max_retries: i32,
) {
    use crate::schema::background_jobs::dsl;
    use diesel::dsl::now;

    let status = if job_retries >= max_retries {
        JobStatus::Failed.to_string()
    } else {
        JobStatus::FailedReQueued.to_string()
    };

    let _ = update(dsl::background_jobs.find(job_id))
        .set((
            dsl::status.eq(status),
            dsl::retries.eq(dsl::retries + 1),
            dsl::last_retry_at.eq(now),
        ))
        .execute(conn);
}
