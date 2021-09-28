use crate::PerformError;

pub enum Event {
    PanicOccurred(PerformError),
    ErrorLoadingJob(diesel::result::Error),
    FailedToAcquireConnection(deadpool_diesel::PoolError),
    // TaskExecutionFailed is created when a task cannot be started.
    TaskExecutionFailed(tokio::task::JoinError),
    // TxnInternal is created when there's an error processing the transaction.
    TxnInternal(diesel::result::Error),
}

impl From<diesel::result::Error> for Event {
    fn from(err: diesel::result::Error) -> Self {
        Event::TxnInternal(err)
    }
}

impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            // Event::Working => f.debug_struct("Working").finish(),
            // Event::NoJobAvailable => f.debug_struct("NoJobAvailable").finish(),
            // Event::TaskFailed(e) => f.debug_tuple("TaskFailed").field(e).finish(),
            Event::PanicOccurred(e) => f.debug_tuple("PanicOccurred").field(e).finish(),
            Event::ErrorLoadingJob(e) => f.debug_tuple("ErrorLoadingJob").field(e).finish(),
            Event::FailedToAcquireConnection(e) => {
                f.debug_tuple("FailedToAcquireConnection").field(e).finish()
            }
            Event::TaskExecutionFailed(e) => f.debug_tuple("TaskExecutionFailed").field(e).finish(),
            Event::TxnInternal(e) => f.debug_tuple("TxnInternal").field(e).finish(),
        }
    }
}

impl std::fmt::Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        <Self as std::fmt::Debug>::fmt(self, f)
    }
}

impl std::error::Error for Event {}
