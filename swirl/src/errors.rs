use std::error::Error;
use std::fmt;
use std::sync::PoisonError;

use diesel::result::Error as DieselError;

pub use FailedJobsError::JobsFailed;
pub use FailedJobsError::PanicOccurred;

/// An error that occurred queueing a job.
#[derive(Debug)]
pub enum EnqueueError {
    /// An error occurred serializing the job
    SerializationError(serde_json::error::Error),

    /// An error occurred inserting the job into the database
    DatabaseError(DieselError),
}

impl From<serde_json::error::Error> for EnqueueError {
    fn from(e: serde_json::error::Error) -> Self {
        EnqueueError::SerializationError(e)
    }
}

impl From<DieselError> for EnqueueError {
    fn from(e: DieselError) -> Self {
        EnqueueError::DatabaseError(e)
    }
}

impl fmt::Display for EnqueueError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            EnqueueError::SerializationError(e) => e.fmt(f),
            EnqueueError::DatabaseError(e) => e.fmt(f),
        }
    }
}

impl Error for EnqueueError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            EnqueueError::SerializationError(e) => Some(e),
            EnqueueError::DatabaseError(e) => Some(e),
        }
    }
}

/// An error that occurred performing a job.
// pub type PerformError = Box<dyn Error>;
// pub type PerformError = Box<dyn Error + Send>;
pub type PerformError = String;

/// An error occurred while attempting to fetch jobs from the queue
// pub enum RunningError {
//     /// We could not acquire a database connection from the pool.
//     ///
//     /// Either the connection pool is too small, or new connections cannot be
//     /// established.
//     NoDatabaseConnection(deadpool_diesel::PoolError),
//
//     /// Could not execute the query to load a job from the database.
//     FailedLoadingJob(DieselError),
//
//     JobPanicked(PerformError),
// }
//
// impl fmt::Debug for FetchError {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         match self {
//             FetchError::NoDatabaseConnection(e) => {
//                 f.debug_tuple("NoDatabaseConnection").field(e).finish()
//             }
//             FetchError::FailedLoadingJob(e) => f.debug_tuple("FailedLoadingJob").field(e).finish(),
//             FetchError::NoMessageReceived => f.debug_struct("NoMessageReceived").finish(),
//         }
//     }
// }

/*
impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FetchError::NoDatabaseConnection(e) => {
                write!(f, "Timed out acquiring a database connection. ")?;
                write!(f, "Try increasing the connection pool size: ")?;
                write!(f, "{}", e)?;
            }
            FetchError::FailedLoadingJob(e) => {
                write!(f, "An error occurred loading a job from the database: ")?;
                write!(f, "{}", e)?;
            }
            FetchError::NoMessageReceived => {
                write!(f, "No message was received from the worker thread. ")?;
                write!(f, "Try increasing the thread pool size or timeout period.")?;
            }
        }
        Ok(())
    }
}

impl Error for FetchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            FetchError::NoDatabaseConnection(e) => Some(e),
            FetchError::FailedLoadingJob(e) => Some(e),
            FetchError::NoMessageReceived => None,
        }
    }
}
*/
/// An error returned by `Runner::check_for_failed_jobs`. Only used in tests.
#[derive(Debug)]
pub enum FailedJobsError {
    /// Jobs failed to run
    JobsFailed(
        /// The number of failed jobs
        i64,
    ),

    PanicOccurred,

    #[doc(hidden)]
    /// Match on `_` instead, more variants may be added in the future
    /// Some other error occurred. Worker threads may have panicked, an error
    /// occurred counting failed jobs in the DB, or something else
    /// unexpectedly went wrong.
    __Unknown(Box<dyn Error + Send + Sync>),
}

impl From<Box<dyn Error + Send + Sync>> for FailedJobsError {
    fn from(e: Box<dyn Error + Send + Sync>) -> Self {
        FailedJobsError::__Unknown(e)
    }
}

impl From<deadpool_diesel::PoolError> for FailedJobsError {
    fn from(e: deadpool_diesel::PoolError) -> Self {
        FailedJobsError::__Unknown(e.into())
    }
}

impl From<DieselError> for FailedJobsError {
    fn from(e: DieselError) -> Self {
        FailedJobsError::__Unknown(e.into())
    }
}

impl<T> From<std::sync::PoisonError<T>> for FailedJobsError {
    fn from(_: PoisonError<T>) -> Self {
        FailedJobsError::PanicOccurred
    }
}

impl PartialEq for FailedJobsError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (JobsFailed(x), JobsFailed(y)) => x == y,
            _ => false,
        }
    }
}

impl fmt::Display for FailedJobsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use FailedJobsError::*;

        match self {
            JobsFailed(x) => write!(f, "{} jobs failed", x),
            PanicOccurred => write!(f, "A panic occurred while executing the job"),
            FailedJobsError::__Unknown(e) => e.fmt(f),
        }
    }
}

impl Error for FailedJobsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            JobsFailed(_) => None,
            PanicOccurred => None,
            FailedJobsError::__Unknown(e) => Some(&**e),
        }
    }
}
