use std::panic::{catch_unwind, AssertUnwindSafe, PanicInfo, RefUnwindSafe, UnwindSafe};
use std::sync::Arc;
use std::time::Duration;

use diesel::prelude::*;
use futures::stream::FuturesUnordered;

use event::*;

use crate::errors::*;
use crate::{storage, Registry};

mod event;

pub struct NoConnectionPoolGiven;

#[allow(missing_debug_implementations)]
pub struct Builder<Env> {
    connection_pool: deadpool_diesel::postgres::Pool,
    environment: Env,
    concurrency: Option<usize>,
    job_timeout: Option<Duration>,
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
    /// Defaults to 10 seconds.
    pub fn job_timeout(mut self, timeout: Duration) -> Self {
        self.job_timeout = Some(timeout);
        self
    }

    /// Provide a connection pool to be used by the runner.
    pub fn connection_pool(self, pool: deadpool_diesel::postgres::Pool) -> Builder<Env> {
        Builder {
            connection_pool: pool,
            environment: self.environment,
            concurrency: self.concurrency,
            job_timeout: self.job_timeout,
        }
    }
}

impl<Env> Builder<Env> {
    /// Build the runner
    pub fn build(self) -> Runner<Env> {
        Runner {
            connection_pool: self.connection_pool,
            environment: Arc::new(self.environment),
            registry: Arc::new(Registry::load()),
            concurrency: self.concurrency.unwrap_or(5),
            job_timeout: self.job_timeout.unwrap_or(Duration::from_secs(300)),
        }
    }
}

#[allow(missing_debug_implementations)]
/// The core runner responsible for locking and running jobs.
pub struct Runner<Env: 'static> {
    connection_pool: deadpool_diesel::postgres::Pool,
    environment: Arc<Env>,
    registry: Arc<Registry<Env>>,
    concurrency: usize,
    job_timeout: Duration,
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
            concurrency: None,
            job_timeout: None,
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
    /// thread will have tried to acquire a new job and found there were none in
    /// the queue.
    ///
    /// New: Stop queueing more jobs as soon as an error occurs.
    ///
    /// Trade-off: Give up some potential for saturating worker threads so that we
    /// can handle every error.
    // pub async fn run_all_pending_jobs(&self) -> Result<(), FetchError> {
    pub async fn run_all_pending_jobs(&'static self) -> Result<(), Event> {
        use futures::StreamExt;

        let max_threads = self.concurrency;
        // let worker_semaphore = Semaphore::new(max_threads);

        // let (sender, receiver) = channel::new(max_threads);
        // let mut pending_messages = 0;

        // let (err_sender, err_receiver) = channel::new(max_threads);

        // let mut fut = futures::future::ready::<usize>(1);
        let mut async_tasks = FuturesUnordered::new();

        // let mut err: std::sync::Arc<std::sync::Mutex<Option<()>>> =
        //     std::sync::Arc::new(std::sync::Mutex::new(None));

        // let has_error = std::sync::Arc::new(AtomicBool::new(false));
        // let has_error1 = has_error.clone();
        // let has_error2 = has_error.clone();

        let do_job = || {
            tokio::task::spawn_blocking(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(tokio::time::timeout(
                        self.job_timeout.clone(), // TODO: configurable
                        self.run_single_job(),
                    ))
            })
        };

        let mut err: Option<Event> = None;

        // let t1 = tokio::spawn(async move {
        //     let t1 = tokio::spawn(async move {
        loop {
            // let has_err = ec1.load(Ordering::SeqCst);
            let running_tasks = async_tasks.len();

            tokio::select! {
                biased;

                // _ = async {}, if has_err && running_tasks == 0 => {
                _ = async {}, if err.is_some() && running_tasks == 0 => {
                    println!("Shutting down");
                    return Err(err.unwrap());
                    // break;
                }
                // _ = async {}, if !has_err && running_tasks < max_threads => {
                _ = async {}, if err.is_none() && running_tasks < max_threads => {
                    async_tasks.push(do_job());
                }
                job_run_res = async_tasks.select_next_some() => {
                    // Don't start any more job executions if an error has occurred.
                    match job_run_res {
                        Err(join_err) => {
                            err = Some(Event::TaskExecutionFailed(join_err));
                        }
                        Ok(job_timeout_res) => {
                            match job_timeout_res {
                                Ok(job_res) => {
                                    match job_res {
                                        Ok(_) => {
                                            async_tasks.push(do_job());
                                        },
                                        Err(e) => {
                                            err = Some(e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    // The job timed out.
                                    eprintln!("Job timed out: {}", e);
                                    async_tasks.push(do_job());
                                }
                            }
                        }
                    }
                }
            }
        }

        // TODO: Catch CTRL+C and return Ok(())
        // });

        // let t2 = tokio::spawn(async move {
        // match receiver.recv_timeout(self.job_timeout) {
        //     Ok(Event::Working) => pending_messages -= 1,
        //     Ok(Event::NoJobAvailable) => return Ok(()),
        //     Ok(Event::ErrorLoadingJob(e)) => return Err(FetchError::FailedLoadingJob(e)),
        //     Ok(Event::FailedToAcquireConnection(e)) => {
        //         return Err(FetchError::NoDatabaseConnection(e));
        //     }
        //     Ok(Event::TaskExecutionFailed(e)) => {
        //         return Err(FetchError::TaskExecutionFailed(e));
        //     }
        //     Err(_) => return Err(FetchError::NoMessageReceived),
        // }
        //
        // Ok(())
        // });

        // tokio::join!(t1, t2)
        //     .map_err(|e| {
        //         println!("ERR: {:?}", e);
        //
        //         FetchError::TaskExecutionFailed(e);
        //     })
        //     .await
    }

    // async fn run_single_job(&self, sender: EventSender) {
    async fn run_single_job(&self) -> Result<(), Event> {
        let environment = Arc::clone(&self.environment);
        let registry = Arc::clone(&self.registry);
        // let connection_pool = AssertUnwindSafe(self.connection_pool().clone());
        // let connection_pool = &self.connection_pool;
        let connection_pool: AssertUnwindSafe<deadpool_diesel::postgres::Pool> =
            AssertUnwindSafe(self.connection_pool.clone()); // TODO: document why

        self.get_single_job(move |job| {
            let perform_job = registry
                .get(&job.job_type)
                // .ok_or_else(|| PerformError::from(format!("Unknown job type {}", job.job_type)))?;
                .ok_or_else(|| format!("Unknown job type {}", job.job_type))?;
            perform_job.perform(job.data, &environment, connection_pool.clone())
        })
        .await
    }

    // async fn get_single_job<F>(&self, sender: EventSender, f: F)
    async fn get_single_job<F>(&self, f: F) -> Result<(), Event>
    where
        F: FnOnce(storage::BackgroundJob) -> Result<(), PerformError> + Send + UnwindSafe + 'static,
    {
        let conn_pool: deadpool_diesel::postgres::Pool = self.connection_pool.clone();

        // self.thread_pool.execute(move || {
        // let res = tokio::task::spawn_blocking(move || async move {
        let conn_wrapper = match conn_pool.get().await {
            Ok(cw) => cw,
            Err(e) => {
                // sender.send(Event::FailedToAcquireConnection(e));
                return Err(Event::FailedToAcquireConnection(e));
            }
        };

        let mut conn = match conn_wrapper.lock() {
            Ok(conn) => conn,
            Err(_e) => {
                // sender
                //     .send(Event::FailedToAcquireConnection(deadpool_diesel::PoolError::Closed));
                // return;
                return Err(Event::FailedToAcquireConnection(deadpool_diesel::PoolError::Closed));
            }
        };

        // #[derive(Debug)]
        // pub enum TxnError {
        //     // TxnInternal is created when there's an error processing the transaction.
        //     TxnInternal(diesel::result::Error),
        //     // Perform is created when there's an error within the task.
        //     Perform(PerformError),
        // }

        // impl From<diesel::result::Error> for TxnError {
        //     fn from(err: diesel::result::Error) -> Self {
        //         TxnError::TxnInternal(err)
        //     }
        // }

        // let job_run_result = conn.transaction::<(), Event, _>(|conn| {
        conn.transaction::<(), Event, _>(|conn| {
            let job = match storage::find_next_unlocked_job(conn).optional() {
                Ok(Some(j)) => {
                    // sender.send(Event::Working);
                    j
                }
                Ok(None) => {
                    // sender.send(Event::NoJobAvailable);
                    return Ok(());
                }
                Err(e) => {
                    // sender.send(Event::ErrorLoadingJob(e));
                    // return Err(RollbackTransaction);
                    return Err(Event::ErrorLoadingJob(e));
                }
            };

            let job_id = job.id.clone();

            let r1 = catch_unwind(|| f(job)).map_err(|e| try_to_extract_panic_info(&e));
            match r1 {
                Err(e) => {
                    // Panic occurred. No more jobs will be started.
                    eprintln!("Job {} panicked: {:?}", job_id, e);
                    storage::update_failed_job(conn, &job_id);
                    Err(Event::PanicOccurred(e))
                }
                Ok(perform_res) => {
                    // The job still could have failed, but we'll re-queue it and carry on.
                    match perform_res {
                        Ok(_) => {
                            if let Err(e) = storage::delete_successful_job(conn, &job_id) {
                                // TODO
                                eprintln!("Could not update job {} as succeeded: {}", job_id, e);
                            }
                            Ok(())
                        }
                        Err(e) => {
                            eprintln!("Job {} failed: {}", job_id, e);
                            storage::update_failed_job(conn, &job_id);
                            Ok(())
                        }
                    }
                }
            }

            // let result = catch_unwind(|| f(job))
            //     .map_err(|e| try_to_extract_panic_info(&e))
            // let result = r1
            //     .and_then(|r| r);
            //
            // match result {
            //     Ok(_) => storage::delete_successful_job(conn, &job_id),
            //     Err(e) => {
            //         eprintln!("Job {} failed: {}", job_id, e);
            //         storage::update_failed_job(conn, &job_id);
            //         Err(e)
            //     }
            // }
        })

        // match job_run_result {
        //     Ok(_) | Err(Event::TaskFailed(_)) => Ok(()),
        //     // Ok(_) | Err(RollbackTransaction) => Ok(()),
        //     Err(e) => {
        //         Err(e)
        //         // panic!("Failed to update job: {:?}", e);
        //     }
        // }
        // })
        // .await;
        //
        // if let Err(e) = res {
        //     sender.send(Event::TaskExecutionFailed(e));
        //     return;
        // }
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
    use std::sync::{Arc, Barrier, Mutex, MutexGuard};

    use diesel::prelude::*;

    use crate::schema::background_jobs::dsl::*;

    use super::*;

    /*
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

    type Runner<Env> = crate::Runner<Env>;

    fn runner() -> Runner<()> {
        let database_url =
            dotenv::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set to run tests");

        let pool_manager = deadpool_diesel::postgres::Manager::new(
            &database_url,
            deadpool_diesel::Runtime::Tokio1,
        );

        let pool = deadpool_diesel::postgres::Pool::builder(pool_manager)
            .max_size(5)
            .build()
            .unwrap();

        crate::Runner::builder((), pool).concurrency(2).build()
    }

    fn create_dummy_job(runner: &Runner<()>) -> storage::BackgroundJob {
        ::diesel::insert_into(background_jobs)
            .values((job_type.eq("Foo"), data.eq(serde_json::json!(null))))
            .returning((id, job_type, data))
            .get_result(&*runner.connection().unwrap())
            .unwrap()
    }
}
