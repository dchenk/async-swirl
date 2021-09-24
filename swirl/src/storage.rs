use diesel::pg::Pg;
use diesel::prelude::*;
use diesel::sql_types;
use diesel::{delete, insert_into, update};
use serde_json;

use crate::errors::EnqueueError;
use crate::schema::background_jobs;
use crate::Job;

#[derive(Queryable, Identifiable, Debug, Clone)]
pub struct BackgroundJob {
    pub id: String,
    pub job_type: String,
    pub data: serde_json::Value,
}

/// Enqueues a job to be run as soon as possible.
pub fn enqueue_job<T: Job>(conn: &mut PgConnection, job: T) -> Result<(), EnqueueError> {
    use crate::schema::background_jobs::dsl::*;

    let job_data = serde_json::to_value(job)?;
    insert_into(background_jobs)
        .values((job_type.eq(T::JOB_TYPE), data.eq(job_data)))
        .execute(conn)?;
    Ok(())
}

fn retriable() -> Box<dyn BoxableExpression<background_jobs::table, Pg, SqlType = sql_types::Bool>>
{
    use crate::schema::background_jobs::dsl as jobsDsl;
    use diesel::dsl::{now, IntervalDsl};

    sql_function!(fn power(x: sql_types::Integer, y: sql_types::Integer) -> sql_types::Integer);
    sql_function!(fn to_timestamp(x: sql_types::Integer) -> sql_types::Timestamp);
    sql_function!(fn coalesce(x: sql_types::Nullable<sql_types::Timestamp>, y: sql_types::Timestamp) -> sql_types::Timestamp);

    Box::new(
        jobsDsl::last_retry_at
            .is_null()
            .or(coalesce(jobsDsl::last_retry_at, to_timestamp(0)).lt(
                now - 1.minute().into_sql::<sql_types::Interval>() * power(2, jobsDsl::retries)
            )),
    )
}

/// Finds the next job that is unlocked, and ready to be retried. If a row is
/// found, it will be locked.
pub fn find_next_unlocked_job(conn: &mut PgConnection) -> QueryResult<BackgroundJob> {
    use crate::schema::background_jobs::dsl::*;

    background_jobs
        .select((id, job_type, data))
        .filter(retriable())
        .order(id)
        .for_update()
        .skip_locked()
        .first::<BackgroundJob>(conn)
}

/// The number of jobs that have failed at least once
pub fn failed_job_count(conn: &mut PgConnection) -> QueryResult<i64> {
    use crate::schema::background_jobs::dsl::*;

    background_jobs.count().filter(retries.gt(0)).get_result(conn)
}

/// Deletes a job that has successfully completed running
pub fn delete_successful_job(conn: &mut PgConnection, job_id: &String) -> QueryResult<()> {
    use crate::schema::background_jobs::dsl::*;

    delete(background_jobs.find(job_id)).execute(conn)?;
    Ok(())
}

/// Marks that we just tried and failed to run a job.
///
/// Ignores any database errors that may have occurred. If the DB has gone away,
/// we assume that just trying again with a new connection will succeed.
pub fn update_failed_job(conn: &mut PgConnection, job_id: &String) {
    use crate::schema::background_jobs::dsl;
    use diesel::dsl::now;

    let _ = update(dsl::background_jobs.find(job_id))
        .set((dsl::retries.eq(dsl::retries + 1), dsl::last_retry_at.eq(now)))
        .execute(conn);
}
