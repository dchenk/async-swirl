#![allow(clippy::new_without_default)] // https://github.com/rust-lang/rust-clippy/issues/3632

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::panic::UnwindSafe;

use crate::errors::PerformError;
use crate::Job;

#[derive(Default)]
#[allow(missing_debug_implementations)] // Can't derive debug
/// A registry of background jobs, used to map job types to concrete perform
/// functions at runtime.
pub struct Registry<Env: UnwindSafe + Send + 'static> {
    jobs: HashMap<&'static str, JobVTable>,
    _marker: PhantomData<Env>,
}

impl<Env: UnwindSafe + Send + 'static> Registry<Env> {
    /// Loads the registry from all invocations of [`register_job!`] for this
    /// environment type
    pub fn load() -> Self {
        let jobs = inventory::iter::<JobVTable>
            .into_iter()
            .filter(|vtable| vtable.env_type == TypeId::of::<Env>())
            .map(|vtable| (vtable.job_type, vtable.clone()))
            .collect();

        Self {
            jobs,
            _marker: PhantomData,
        }
    }

    /// Get the perform function for a given job type
    pub fn get(&self, job_type: &str) -> Option<PerformJob<Env>> {
        self.jobs.get(job_type).map(|vtable| PerformJob {
            vtable: vtable.clone(),
            _marker: PhantomData,
        })
    }
}

/// Register a job to be run by swirl. This must be called for any
/// implementors of [`swirl::Job`]
#[macro_export]
macro_rules! register_job {
    ($job_ty: ty) => {
        $crate::inventory::submit! {
            #![crate = swirl]
            swirl::JobVTable::from_job::<$job_ty>()
        }
    };
}

#[doc(hidden)]
#[derive(Clone)]
pub struct JobVTable {
    env_type: TypeId,
    job_type: &'static str,
    perform: fn(
        serde_json::Value,
        &dyn Any,
        deadpool_diesel::postgres::Pool,
    ) -> futures::future::BoxFuture<'_, Result<(), PerformError>>,
}

inventory::collect!(JobVTable);

impl JobVTable {
    pub fn from_job<T: Job + Send + Sync + 'static>() -> Self {
        Self {
            env_type: TypeId::of::<T::Environment>(),
            job_type: T::JOB_TYPE,
            perform: move |d, e, p| {
                let env_opt: Option<&T::Environment> = e.downcast_ref();
                Box::pin(perform_job::<T>(d, env_opt, p))
            },
        }
    }
}

async fn perform_job<T: Job>(
    data: serde_json::Value,
    env: Option<&T::Environment>,
    pool: deadpool_diesel::postgres::Pool,
) -> Result<(), PerformError> {
    let environment = env.ok_or_else::<PerformError, _>(|| {
        "Incorrect environment type. This should never happen. \
             Please open an issue at https://github.com/dchenk/async-swirl/issues/new"
            .into()
    })?;
    let data = serde_json::from_value(data).map_err(|e| format!("{:?}", e))?;
    T::perform(data, environment, pool).await
}

pub struct PerformJob<Env: UnwindSafe> {
    vtable: JobVTable,
    _marker: PhantomData<Env>,
}

impl<Env: UnwindSafe + Send + Sync + 'static> PerformJob<Env> {
    pub async fn perform(
        &self,
        data: serde_json::Value,
        env: &Env,
        pool: deadpool_diesel::postgres::Pool,
    ) -> Result<(), PerformError> {
        let perform_fn = self.vtable.perform;
        perform_fn(data, env, pool).await
    }
}
