use diesel::result::Error as DieselError;

use super::channel;

pub type EventSender = channel::Sender<Event>;

pub enum Event {
    Working,
    NoJobAvailable,
    ErrorLoadingJob(DieselError),
    FailedToAcquireConnection(deadpool_diesel::PoolError),
}

impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Event::Working => f.debug_struct("Working").finish(),
            Event::NoJobAvailable => f.debug_struct("NoJobAvailable").finish(),
            Event::ErrorLoadingJob(e) => f.debug_tuple("ErrorLoadingJob").field(e).finish(),
            Event::FailedToAcquireConnection(e) => {
                f.debug_tuple("FailedToAcquireConnection").field(e).finish()
            }
        }
    }
}
