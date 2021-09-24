use std::error::Error;
use std::ops::DerefMut;
use std::panic::{catch_unwind, AssertUnwindSafe, PanicInfo, RefUnwindSafe, UnwindSafe};
use std::sync::Arc;
use std::time::Duration;

use diesel::prelude::*;
use threadpool::ThreadPool;

use event::*;

use crate::errors::*;
use crate::{storage, Registry};

mod channel;
mod event;

pub struct NoConnectionPoolGiven;

#[allow(missing_debug_implementations)]
pub struct Builder<Env> {
    connection_pool: deadpool_diesel::postgres::Pool,
    environment: Env,
    thread_count: Option<usize>,
    job_start_timeout: Option<Duration>,
}

impl<Env> Builder<Env> {
    /// Set the number of threads to be used to run jobs concurrently.
    ///
    /// Defaults to 5
    pub fn thread_count(mut self, thread_count: usize) -> Self {
        self.thread_count = Some(thread_count);
        self
    }

    fn get_thread_count(&self) -> usize {
        self.thread_count.unwrap_or(5)
    }

    /// The amount of time to wait for a job to start before assuming an error
    /// has occurred.
    ///
    /// Defaults to 10 seconds.
    pub fn job_start_timeout(mut self, timeout: Duration) -> Self {
        self.job_start_timeout = Some(timeout);
        self
    }

    /// Provide a connection pool to be used by the runner.
    pub fn connection_pool(self, pool: deadpool_diesel::postgres::Pool) -> Builder<Env> {
        Builder {
            connection_pool: pool,
            environment: self.environment,
            thread_count: self.thread_count,
            job_start_timeout: self.job_start_timeout,
        }
    }
}

impl<Env> Builder<Env> {
    /// Build the runner
    pub fn build(self) -> Runner<Env> {
        Runner {
            thread_pool: ThreadPool::new(self.get_thread_count()),
            connection_pool: self.connection_pool,
            environment: Arc::new(self.environment),
            registry: Arc::new(Registry::load()),
            job_start_timeout: self.job_start_timeout.unwrap_or(Duration::from_secs(10)),
        }
    }
}

#[allow(missing_debug_implementations)]
/// The core runner responsible for locking and running jobs.
pub struct Runner<Env: 'static> {
    connection_pool: deadpool_diesel::postgres::Pool,
    thread_pool: ThreadPool,
    environment: Arc<Env>,
    registry: Arc<Registry<Env>>,
    job_start_timeout: Duration,
}

impl<Env> Runner<Env> {
    /// Create a builder for a job runner
    ///
    /// This method takes the two required configurations: the database
    /// connection pool, and the environment to pass to your jobs. If your
    /// environment contains a connection pool, it should be the same pool given
    /// here.
    pub fn builder(
        environment: Env,
        connection_pool: deadpool_diesel::postgres::Pool,
    ) -> Builder<Env> {
        Builder {
            connection_pool,
            environment,
            thread_count: None,
            job_start_timeout: None,
        }
    }
}

impl<Env> Runner<Env>
where
    Env: RefUnwindSafe + Send + Sync + 'static,
{
    /// Runs all pending jobs in the queue.
    ///
    /// This function will return once all jobs in the queue have begun running but
    /// does not wait for them to complete. When this function returns, at least one
    /// thread will have tried to acquire a new job, and found there were none in
    /// the queue.
    pub async fn run_all_pending_jobs(&self) -> Result<(), FetchError> {
        use std::cmp::max;

        let max_threads = self.thread_pool.max_count();
        let (sender, receiver) = channel::new(max_threads);
        let mut pending_messages = 0;
        loop {
            let available_threads = max_threads - self.thread_pool.active_count();

            let jobs_to_queue = if pending_messages == 0 {
                // If we have no queued jobs talking to us, and there are no
                // available threads, we still need to queue at least one job
                // or we'll never receive a message
                max(available_threads, 1)
            } else {
                available_threads
            };

            for _ in 0..jobs_to_queue {
                self.run_single_job(sender.clone()).await;
            }

            pending_messages += jobs_to_queue;
            match receiver.recv_timeout(self.job_start_timeout) {
                Ok(Event::Working) => pending_messages -= 1,
                Ok(Event::NoJobAvailable) => return Ok(()),
                Ok(Event::ErrorLoadingJob(e)) => return Err(FetchError::FailedLoadingJob(e)),
                Ok(Event::FailedToAcquireConnection(e)) => {
                    return Err(FetchError::NoDatabaseConnection(e));
                }
                Err(_) => return Err(FetchError::NoMessageReceived),
            }
        }
    }

    async fn run_single_job(&self, sender: EventSender) {
        let environment = Arc::clone(&self.environment);
        let registry = Arc::clone(&self.registry);
        // let connection_pool = AssertUnwindSafe(self.connection_pool().clone());
        // let connection_pool = &self.connection_pool;
        let connection_pool: AssertUnwindSafe<deadpool_diesel::postgres::Pool> =
            AssertUnwindSafe(self.connection_pool.clone()); // TODO
        self.get_single_job(sender, move |job| {
            let perform_job = registry
                .get(&job.job_type)
                .ok_or_else(|| PerformError::from(format!("Unknown job type {}", job.job_type)))?;
            perform_job.perform(job.data, &environment, connection_pool.clone())
        })
        .await
    }

    async fn get_single_job<F>(&self, sender: EventSender, f: F)
    where
        F: FnOnce(storage::BackgroundJob) -> Result<(), PerformError> + Send + UnwindSafe + 'static,
    {
        use diesel::result::Error::RollbackTransaction;

        // The connection may not be `Send` so we need to clone the pool instead
        // let pool = self.connection_pool.clone();
        // let conn_wrapper =
        //     self.connection_pool.get().await.map_err(Into::<FailedJobsError>::into)?;
        // let mut conn_guard = conn_wrapper.lock().map_err(|_e| FailedJobsError::PanicOccurred)?;
        // let mut conn = conn_guard.deref_mut();
        // let mut conn: &mut PgConnection = match self.connection_pool.get().await {
        let conn_wrapper = match self.connection_pool.get().await {
            Ok(cw) => cw,
            // Ok(conn_wrapper) => match conn_wrapper.lock() {
            //     Ok(conn) => conn.deref_mut(),
            //     Err(e) => {
            //         sender
            //             .send(Event::FailedToAcquireConnection(deadpool_diesel::PoolError::Closed));
            //         return;
            //     }
            // },
            Err(e) => {
                sender.send(Event::FailedToAcquireConnection(e));
                return;
            }
        };

        self.thread_pool.execute(move || {
            let mut conn = match conn_wrapper.lock() {
                Ok(conn) => conn,
                Err(_e) => {
                    sender
                        .send(Event::FailedToAcquireConnection(deadpool_diesel::PoolError::Closed));
                    return;
                }
            };
            // let conn = match pool.get().await {
            //     Ok(conn_wrapper) => match conn_wrapper.lock() {
            //         Ok(conn) => conn,
            //         Err(e) => {
            //             sender.send(Event::FailedToAcquireConnection(deadpool_diesel::PoolError::Closed));
            //             return;
            //         }
            //     },
            //     Err(e) => {
            //         sender.send(Event::FailedToAcquireConnection(e));
            //         return;
            //     }
            // };

            let job_run_result = conn.transaction::<_, diesel::result::Error, _>(|conn| {
                let job = match storage::find_next_unlocked_job(conn).optional() {
                    Ok(Some(j)) => {
                        sender.send(Event::Working);
                        j
                    }
                    Ok(None) => {
                        sender.send(Event::NoJobAvailable);
                        return Ok(());
                    }
                    Err(e) => {
                        sender.send(Event::ErrorLoadingJob(e));
                        return Err(RollbackTransaction);
                    }
                };

                let job_id = job.id.clone();

                let result = catch_unwind(|| f(job))
                    .map_err(|e| try_to_extract_panic_info(&e))
                    .and_then(|r| r);

                match result {
                    Ok(_) => storage::delete_successful_job(conn, &job_id)?,
                    Err(e) => {
                        eprintln!("Job {} failed to run: {}", job_id, e);
                        storage::update_failed_job(conn, &job_id);
                    }
                }
                Ok(())
            });

            match job_run_result {
                Ok(_) | Err(RollbackTransaction) => {}
                Err(e) => {
                    panic!("Failed to update job: {:?}", e);
                }
            }
        })
    }

    /// Waits for all running jobs to complete and returns an error if any failed.
    ///
    /// This function is intended for use in tests. If any jobs have failed, it
    /// will return `swirl::JobsFailed` with the number of jobs that failed.
    ///
    /// If any other unexpected errors occurred, such as panicked worker threads
    /// or an error loading the job count from the database, an opaque error
    /// will be returned.
    pub async fn check_for_failed_jobs(&self) -> Result<(), FailedJobsError> {
        self.wait_for_jobs()?;
        let conn_wrapper =
            self.connection_pool.get().await.map_err(Into::<FailedJobsError>::into)?;
        let mut conn_guard = conn_wrapper.lock().map_err(|_e| FailedJobsError::PanicOccurred)?;
        let mut conn = conn_guard.deref_mut();

        let failed_jobs = storage::failed_job_count(&mut conn)?;
        if failed_jobs == 0 {
            Ok(())
        } else {
            Err(JobsFailed(failed_jobs))
        }
    }

    fn wait_for_jobs(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        self.thread_pool.join();
        let panic_count = self.thread_pool.panic_count();
        if panic_count == 0 {
            Ok(())
        } else {
            Err(format!("{} threads panicked", panic_count).into())
        }
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
        format!("job panicked: {}", x).into()
    } else if let Some(x) = info.downcast_ref::<&'static str>() {
        format!("job panicked: {}", x).into()
    } else if let Some(x) = info.downcast_ref::<String>() {
        format!("job panicked: {}", x).into()
    } else {
        "job panicked".into()
    }
}

#[cfg(test)]
mod tests {
    use std::panic::AssertUnwindSafe;
    use std::sync::{Arc, Barrier, Mutex, MutexGuard};

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

        runner.get_single_job(channel::dummy_sender(), move |job| {
            fetch_barrier.0.wait(); // Tell thread 2 it can lock its job
            assert_eq!(first_job_id, job.id);
            return_barrier.0.wait(); // Wait for thread 2 to lock its job
            Ok(())
        });

        fetch_barrier2.0.wait(); // Wait until thread 1 locks its job
        runner.get_single_job(channel::dummy_sender(), move |job| {
            assert_eq!(second_job_id, job.id);
            return_barrier2.0.wait(); // Tell thread 1 it can unlock its job
            Ok(())
        });

        runner.wait_for_jobs().unwrap();
    }

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

    // TODO!!!!!
    /*
    impl<'a> Drop for TestGuard<'a> {
        fn drop(&mut self) {
            ::diesel::sql_query("TRUNCATE TABLE background_jobs")
                .execute(&*runner().connection().unwrap())
                .unwrap();
        }
    }
     */

    type Runner<Env> = crate::Runner<Env>;

    fn runner() -> Runner<()> {
        let database_url =
            dotenv::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set to run tests");

        crate::Runner::builder(()).database_url(database_url).thread_count(2).build()
    }

    fn create_dummy_job(runner: &Runner<()>) -> storage::BackgroundJob {
        ::diesel::insert_into(background_jobs)
            .values((job_type.eq("Foo"), data.eq(serde_json::json!(null))))
            .returning((id, job_type, data))
            .get_result(&*runner.connection().unwrap())
            .unwrap()
    }
}
