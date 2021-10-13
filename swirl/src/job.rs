use async_trait::async_trait;
use diesel::PgConnection;
use serde::{de::DeserializeOwned, Serialize};

use crate::errors::{EnqueueError, PerformError};
use crate::storage;

/// A background job, meant to be run asynchronously.
#[async_trait]
pub trait Job: Serialize + DeserializeOwned {
    /// The environment this job is run with. This is a struct you define,
    /// which should encapsulate things like database connection pools, any
    /// configuration, and any other static data or shared resources.
    type Environment: Sync + 'static;

    /// The key to use for storing this job, and looking it up later.
    ///
    /// Typically this is the name of your struct in `snake_case`
    const JOB_TYPE: &'static str;

    /// Enqueue this job to be run at some point in the future.
    fn enqueue(self, conn: &mut PgConnection) -> Result<(), EnqueueError> {
        storage::enqueue_job(conn, self)
    }

    /// The logic involved in performing this job.
    async fn perform(
        self,
        env: &Self::Environment,
        pool: deadpool_diesel::postgres::Pool,
    ) -> Result<(), PerformError>;
}
