use std::panic::{AssertUnwindSafe, PanicInfo, RefUnwindSafe, UnwindSafe};
use std::sync::Arc;
use std::time::Duration;

use diesel::prelude::*;

use crate::{DieselPool, Registry, storage};
use crate::errors::*;
use crate::registry::PerformJob;
use diesel::connection::{AnsiTransactionManager, TransactionManager};

pub struct NoConnectionPoolGiven;

#[allow(missing_debug_implementations)]
pub struct Builder<Env> {
    connection_pool: deadpool_diesel::postgres::Pool,
    environment: Env,
    concurrency: Option<usize>,
    job_timeout: Option<Duration>,
    max_retries: Option<usize>,
}

impl<Env> Builder<Env> {
    /// Set the number of threads to be used to run jobs concurrently.
    ///
    /// Defaults to 5
    pub fn concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = Some(concurrency);
        self
    }

    /// The amount of time to wait for a job to run before assuming an error
    /// has occurred.
    ///
    /// Defaults to 300 seconds (5 minutes).
    pub fn job_timeout(mut self, timeout: Duration) -> Self {
        self.job_timeout = Some(timeout);
        self
    }

    /// The maximum number of times to retry a job.
    ///
    /// Defaults to 4.
    pub fn max_retries(mut self, max_retries: usize) -> Self {
        self.max_retries = Some(max_retries);
        self
    }
}

impl<Env: UnwindSafe + Send> Builder<Env> {
    /// Build the runner.
    pub fn build(self) -> Runner<Env> {
        Runner {
            connection_pool: self.connection_pool,
            environment: Arc::new(self.environment),
            registry: Arc::new(Registry::load()),
            concurrency: self.concurrency.unwrap_or(5),
            job_timeout: self.job_timeout.unwrap_or(Duration::from_secs(300)),
            max_retries: self.max_retries.unwrap_or(4),
        }
    }
}

enum RunResult {
    JobNotFound(String),
    JobResult(Result<Result<Result<(), Result<Option<PerformError>, JobRunnerError>>, Box<dyn std::any::Any + Send>>, tokio::time::error::Elapsed>)
}

#[allow(missing_debug_implementations)]
/// The core runner responsible for locking and running jobs.
pub struct Runner<Env: UnwindSafe + Send + 'static> {
    connection_pool: deadpool_diesel::postgres::Pool,
    environment: Arc<Env>,
    registry: Arc<Registry<Env>>,
    concurrency: usize,
    job_timeout: Duration,
    max_retries: usize,
}

impl<Env: UnwindSafe + Send> Runner<Env> {
    /// Create a builder for a Runner.
    ///
    /// This method takes the two required configurations: the database connection pool
    /// and the environment to pass to jobs.
    pub fn builder(
        environment: Env,
        connection_pool: deadpool_diesel::postgres::Pool,
    ) -> Builder<Env> {
        Builder {
            connection_pool,
            environment,
            concurrency: None,
            job_timeout: None,
            max_retries: None,
        }
    }
}

impl<Env> Runner<Env>
where
    Env: UnwindSafe + RefUnwindSafe + Send + Sync + 'static,
{
    /// Runs all pending jobs in the queue and continue to pick up and run jobs until an
    /// error processing jobs occurs or until a job panics. This function does not stop
    /// when a normal error occurs within the execution of a job.
    pub async fn start(&self) -> Result<(), JobRunnerError> {
        use futures::{stream::FuturesUnordered, StreamExt, FutureExt};
        // use futures::FutureExt;

        let job_timeout = self.job_timeout.clone();
        let max_retries = self.max_retries as i32;

        let do_job = move |connection_pool: DieselPool,
                           environment: Arc<Env>,
                           registry: Arc<Registry<Env>>| {
            tokio::task::spawn_blocking(move || {
                tokio::runtime::Handle::current().block_on(Self::run_single_job(
                    // job_timeout.clone(),
                    max_retries,
                    connection_pool,
                    // environment,
                    // registry,
                    move |job| {
                        // match registry.get(&job.job_type).ok_or_else(|| {
                        //     // JobRunnerError::UnrecognizedJob(format!(
                        //     //     "Unknown job type {} for job {}; it may be that different jobs have different environment types",
                        //     //     job.job_type, job.id
                        //     // ))
                        //     Ok(
                        //         Ok(
                        //             JobRunnerError::UnrecognizedJob(format!(
                        //                 "Unknown job type {} for job {}; it may be that different jobs have different environment types",
                        //                 job.job_type, job.id
                        //             ))
                        //         )
                        //     )
                        // }) {
                        //    Err(e) => {
                        //        let _x: Result<Result<i32, ()>, ()> = e;
                        //    }
                        //     Ok(v) => {}
                        // };
                        // let perform_job: PerformJob<Env> = registry.get(&job.job_type).ok_or_else(|| -> Result<Result<Result<(), PerformError>, Box<dyn std::any::Any + Send>>, tokio::time::error::Elapsed> {
                        //     // JobRunnerError::UnrecognizedJob(format!(
                        //     //     "Unknown job type {} for job {}; it may be that different jobs have different environment types",
                        //     //     job.job_type, job.id
                        //     // ))
                        //     Result::<_, tokio::time::error::Elapsed>::Ok(
                        //         Ok(
                        //         Err(JobRunnerError::UnrecognizedJob(format!(
                        //             "Unknown job type {} for job {}; it may be that different jobs have different environment types",
                        //             job.job_type, job.id
                        //         )))
                        //         )
                        //     )
                        // })?;
                        let perform_job: PerformJob<Env> = registry.get(&job.job_type).ok_or_else(|| -> Result<Result<Result<(), PerformError>, Box<dyn std::any::Any + Send>>, tokio::time::error::Elapsed> {
                            // JobRunnerError::UnrecognizedJob(format!(
                            //     "Unknown job type {} for job {}; it may be that different jobs have different environment types",
                            //     job.job_type, job.id
                            // ))
                            Result::<_, tokio::time::error::Elapsed>::Ok(
                                Ok(
                                    Err(JobRunnerError::UnrecognizedJob(format!(
                                        "Unknown job type {} for job {}; it may be that different jobs have different environment types",
                                        job.job_type, job.id
                                    )))
                                )
                            )
                        })?;
                        // AssertUnwindSafe(perform_job.perform(
                        //     job.data,
                        //     &environment,
                        //     connection_pool.clone(),
                        // ))
                        // perform_job.perform(
                        //     job.data,
                        //     &environment,
                        //     connection_pool.clone(),
                        // )
                        // futures::executor::block_on(tokio::time::timeout(
                        //     job_timeout,
                        //     AssertUnwindSafe(perform_job.perform(
                        //         job.data,
                        //         &environment,
                        //         connection_pool.clone(),
                        //     ))
                        //     .catch_unwind()
                        // ))
                        tokio::time::timeout(
                            job_timeout,
                            AssertUnwindSafe(perform_job.perform(
                                job.data,
                                &environment,
                                connection_pool.clone(),
                            ))
                                .catch_unwind()
                        )
                        // )) {
                    }
                ))
            })
        };

        let mut async_tasks = FuturesUnordered::new();

        let mut err: Option<JobRunnerError> = None;

        loop {
            let running_tasks = async_tasks.len();

            tokio::select! {
                biased;

                _ = async {}, if err.is_some() && running_tasks == 0 => {
                    eprintln!("Shutting down due to error");
                    return Err(err.unwrap());
                }
                _ = async {}, if err.is_none() && running_tasks < self.concurrency => {
                    async_tasks.push(do_job(self.connection_pool.clone(), self.environment.clone(), self.registry.clone()));
                }
                job_run_res = async_tasks.select_next_some() => {
                    // Don't start any more job executions if an error has occurred.
                    match job_run_res {
                        Err(join_err) => {
                            err = Some(JobRunnerError::TaskExecutionFailed(join_err));
                            async_tasks.push(do_job(self.connection_pool.clone(), self.environment.clone(), self.registry.clone()));
                        }
                        Ok(job_res) => {
                            match job_res {
                                Ok(_) => {
                                    if err.is_none() {
                                        async_tasks.push(do_job(self.connection_pool.clone(), self.environment.clone(), self.registry.clone()));
                                    }
                                },
                                Err(e) => {
                                    err = Some(e);
                                }
                            }
                        }
                    }
                }
            }
        }

        // TODO: Catch CTRL+C and return Ok(())
    }

    async fn run_single_job<F>(
        max_retries: i32,
        connection_pool: DieselPool,
        job_fn: F,
    ) -> Result<(), JobRunnerError>
    where
        // F: FnOnce(storage::BackgroundJob) -> Result<Result<Result<(), Result<Option<PerformError>, JobRunnerError>>, Box<dyn std::any::Any + Send>>, tokio::time::error::Elapsed> + 'static,
        F: FnOnce(storage::BackgroundJob) -> RunResult + 'static,
    {
        let conn_wrapper = match connection_pool.get().await {
            Ok(cw) => cw,
            Err(e) => {
                return Err(JobRunnerError::FailedToAcquireConnection(e));
            }
        };

        let mut conn: &mut PgConnection = match conn_wrapper.lock() {
            Ok(conn) => conn,
            Err(_e) => {
                return Err(JobRunnerError::FailedToAcquireConnection(
                    deadpool_diesel::PoolError::Closed,
                ));
            }
        };

        AnsiTransactionManager::begin_transaction(&mut conn)
        // conn.execute("BEGIN")
            .map_err(|| {
                todo!()
            })?;

        // conn.transaction::<(), JobRunnerError, _>(|conn| {
            let job = match storage::find_next_unlocked_job(conn, max_retries).optional() {
                Ok(Some(j)) => j,
                Ok(None) => {
                    AnsiTransactionManager::commit_transaction(&mut conn)
                        .map_err(|| {
                            todo!()
                        })?;
                    // TODO: Make this sleep time configurable.
                    std::thread::sleep(Duration::from_secs(2));
                    return Ok(());
                }
                Err(e) => {
                    AnsiTransactionManager::rollback_transaction(&mut conn)
                        .map_err(|| {
                            todo!()
                        })?;
                    return Err(JobRunnerError::ErrorLoadingJob(e));
                }
            };

            let job_id = job.id.clone();

            // let perform_job: PerformJob<Env> = registry.get(&job.job_type).ok_or_else(|| {
            //     JobRunnerError::UnrecognizedJob(format!(
            //         "Unknown job type {} for job {}; it may be that different jobs have different environment types",
            //         job.job_type, job_id
            //     ))
            // })?;

            // We don't have a guarantee that the Pool ends up in an internally consistent state
            // after the function that it's passed to panics (i.e., that it is UnwindSafe). Even if
            // it is already safe, we err on the side of caution and deliberately stop processing
            // new jobs whenever a panic occurs.

            let retries = job.retries;

            // match futures::executor::block_on(tokio::time::timeout(
            //     timeout,
            //     // AssertUnwindSafe(perform_job.perform(
            //     //     job.data,
            //     //     &environment,
            //     //     connection_pool.clone(),
            //     // ))
            //     f(job)
            //     .catch_unwind(),
            // )) {
            match job_fn(job) {
                Err(_) => {
                    eprintln!("Job {} timed out", job_id);
                    storage::update_failed_job(conn, &job_id, retries, max_retries);
                    Ok(())
                }
                Ok(perform_res) => {
                    match perform_res {
                        Ok(res) => {
                            if let Err(_) = res {
                                eprintln!("Job {} failed", job_id);
                                storage::update_failed_job(conn, &job_id, retries, max_retries);
                            } else {
                                if let Err(e) = storage::update_successful_job(conn, &job_id) {
                                    eprintln!(
                                        "Could not update job {} as succeeded: {}",
                                        job_id, e
                                    );
                                }
                            }
                            Ok(())
                        }
                        Err(e) => {
                            // Panic occurred. No more jobs will be started.
                            let err_message = try_to_extract_panic_info(&e);
                            eprintln!("Job {} panicked: {:?}", job_id, err_message);
                            storage::update_failed_job(conn, &job_id, retries, max_retries);
                            Err(JobRunnerError::PanicOccurred(err_message))
                        }
                    }
                }
            }
        // })
    }
}

/// Try to figure out what's in the box, and print it if we can.
///
/// The actual error type we will get from `panic::catch_unwind` is really poorly documented.
/// However, the `panic::set_hook` functions deal with a `PanicInfo` type, and its payload is
/// documented as "commonly but not always `&'static str` or `String`". So we can try all of those,
/// and give up if we didn't get one of those three types.
fn try_to_extract_panic_info(info: &(dyn std::any::Any + Send + 'static)) -> PerformError {
    if let Some(x) = info.downcast_ref::<PanicInfo>() {
        format!("job panicked: {}", x)
    } else if let Some(x) = info.downcast_ref::<&'static str>() {
        format!("job panicked: {}", x)
    } else if let Some(x) = info.downcast_ref::<String>() {
        format!("job panicked: {}", x)
    } else {
        String::from("job panicked")
    }
}

#[cfg(test)]
mod tests {
    use std::panic::AssertUnwindSafe;
    use std::sync::{Barrier, Mutex, MutexGuard};

    use diesel::prelude::*;

    use crate::schema::background_jobs::dsl::*;

    use super::*;

    #[test]
    fn jobs_are_locked_when_fetched() {
        let _guard = TestGuard::lock();

        let runner = runner();
        let first_job_id = create_dummy_job(&runner).id;
        let second_job_id = create_dummy_job(&runner).id;
        let fetch_barrier = Arc::new(AssertUnwindSafe(Barrier::new(2)));
        let fetch_barrier2 = fetch_barrier.clone();
        let return_barrier = Arc::new(AssertUnwindSafe(Barrier::new(2)));
        let return_barrier2 = return_barrier.clone();

        let pool = db_pool();

        Runner::run_single_job(std::time::Duration::from_secs(10), 3, pool, Arc::new(()), todo!());

        fetch_barrier2.0.wait(); // Wait until thread 1 locks its job
        runner.get_single_job(channel::dummy_sender(), move |job| {
            assert_eq!(second_job_id, job.id);
            return_barrier2.0.wait(); // Tell thread 1 it can unlock its job
            Ok(())
        });

        runner.wait_for_jobs().unwrap();
    }

    /*
    #[test]
    fn jobs_are_deleted_when_successfully_run() {
        let _guard = TestGuard::lock();

        let runner = runner();
        create_dummy_job(&runner);

        runner.get_single_job(channel::dummy_sender(), |_| Ok(()));
        runner.wait_for_jobs().unwrap();

        let mut conn: PgConnection =
            runner.connection_pool.get().await.unwrap().lock().unwrap().deref_mut();
        let remaining_jobs = background_jobs.count().get_result(&mut conn);
        assert_eq!(Ok(0), remaining_jobs);
    }

    #[test]
    fn failed_jobs_do_not_release_lock_before_updating_retry_time() {
        let _guard = TestGuard::lock();

        let runner = runner();
        create_dummy_job(&runner);
        let barrier = Arc::new(AssertUnwindSafe(Barrier::new(2)));
        let barrier2 = barrier.clone();

        runner.get_single_job(channel::dummy_sender(), move |_| {
            barrier.0.wait();
            // error so the job goes back into the queue
            Err("nope".into())
        });

        let mut conn: PgConnection =
            runner.connection_pool.get().await.unwrap().lock().unwrap().deref_mut();

        // Wait for the first thread to acquire the lock
        barrier2.0.wait();
        // We are intentionally not using `get_single_job` here.
        // `SKIP LOCKED` is intentionally omitted here, so we block until
        // the lock on the first job is released.
        // If there is any point where the row is unlocked, but the retry
        // count is not updated, we will get a row here.
        let available_jobs = background_jobs
            .select(id)
            .filter(retries.eq(0))
            .for_update()
            .load::<i64>(&*conn)
            .unwrap();
        assert_eq!(0, available_jobs.len());

        // Sanity check to make sure the job actually is there
        let total_jobs_including_failed =
            background_jobs.select(id).for_update().load::<i64>(&*conn).unwrap();
        assert_eq!(1, total_jobs_including_failed.len());

        runner.wait_for_jobs().unwrap();
    }

    #[test]
    fn panicking_in_jobs_updates_retry_counter() {
        let _guard = TestGuard::lock();
        let runner = runner();
        let job_id = create_dummy_job(&runner).id;

        runner.get_single_job(channel::dummy_sender(), |_| panic!());
        runner.wait_for_jobs().unwrap();

        let mut conn: PgConnection = runner
            .connection_pool
            .get()
            .await
            .map_err(Into::into)
            .unwrap()
            .lock()
            .map_err(Into::into)
            .unwrap()
            .deref_mut();
        let tries = background_jobs
            .find(job_id)
            .select(retries)
            .for_update()
            .first::<i32>(&*runner.connection().unwrap())
            .unwrap();
        assert_eq!(1, tries);
    }
     */

    lazy_static::lazy_static! {
        // Since these tests deal with behavior concerning multiple connections
        // running concurrently, they have to run outside of a transaction.
        // Therefore we can't run more than one at a time.
        //
        // Rather than forcing the whole suite to be run with `--test-threads 1`,
        // we just lock these tests instead.
        static ref TEST_MUTEX: Mutex<()> = Mutex::new(());
    }

    struct TestGuard<'a>(MutexGuard<'a, ()>);

    impl<'a> TestGuard<'a> {
        fn lock() -> Self {
            TestGuard(TEST_MUTEX.lock().unwrap())
        }
    }

    impl<'a> Drop for TestGuard<'a> {
        fn drop(&mut self) {
            let db_url = dotenv::var("TEST_DATABASE_URL")
                .expect("TEST_DATABASE_URL must be set to run tests");
            let mut conn = PgConnection::establish(&db_url).unwrap();
            ::diesel::sql_query("TRUNCATE TABLE background_jobs")
                .execute(&mut conn)
                .unwrap();
        }
    }

    fn db_pool() -> DieselPool {
        let database_url =
            dotenv::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set to run tests");

        let pool_manager = deadpool_diesel::postgres::Manager::new(
            &database_url,
            deadpool_diesel::Runtime::Tokio1,
        );

        deadpool_diesel::postgres::Pool::builder(pool_manager)
            .max_size(3)
            .build()
            .unwrap()
    }

    fn runner() -> Runner<()> {
        Runner::builder((), db_pool()).concurrency(2).build()
    }

    fn create_dummy_job(runner: &Runner<()>, db_conn: &mut PgConnection) -> storage::BackgroundJob {
        ::diesel::insert_into(background_jobs)
            .values((
                id.eq(min_id::generate_id()),
                job_type.eq("Foo"),
                data.eq(serde_json::json!(null)),
                status.eq("Queued"),
            ))
            .returning((id, job_type, data))
            .get_result(db_conn)
            .unwrap()
    }
}
