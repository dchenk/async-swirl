use antidote::{Mutex, MutexGuard};
use diesel::prelude::*;
use std::ops::{Deref, DerefMut};
use std::time::Duration;
use swirl::{Builder, Runner};

use crate::db::*;
use crate::util::*;

lazy_static::lazy_static! {
    // Since these tests deal with behavior concerning multiple connections
    // running concurrently, they have to run outside of a transaction.
    // Therefore we can't run more than one at a time.
    //
    // Rather than forcing the whole suite to be run with `--test-threads 1`,
    // we just lock these tests instead.
    static ref TEST_MUTEX: Mutex<()> = Mutex::new(());
}

pub struct TestGuard<'a, Env: 'static> {
    runner: Runner<Env>,
    _lock: MutexGuard<'a, ()>,
}

impl<'a, Env> TestGuard<'a, Env> {
    pub fn builder(env: Env) -> GuardBuilder<Env> {
        let builder = Runner::builder(env, build_pool());

        GuardBuilder { builder }
    }

    pub fn runner(env: Env) -> Self {
        Self::builder(env).build()
    }
}

impl<'a> TestGuard<'a, ()> {
    pub fn dummy_runner() -> Self {
        Self::builder(()).build()
    }
}

pub struct GuardBuilder<Env: 'static> {
    builder: Builder<Env>,
}

impl<Env> GuardBuilder<Env> {
    pub fn thread_count(mut self, count: usize) -> Self {
        self.builder = self.builder.thread_count(count);
        self
    }

    pub fn connection_count(mut self, count: u32) -> Self {
        self.builder = self.builder.connection_count(count);
        self
    }

    pub fn job_timeout(mut self, timeout: Duration) -> Self {
        self.builder = self.builder.job_timeout(timeout);
        self
    }

    pub fn build<'a>(self) -> TestGuard<'a, Env> {
        TestGuard {
            _lock: TEST_MUTEX.lock(),
            runner: self.builder.build(),
        }
    }
}

impl<'a, Env> Deref for TestGuard<'a, Env> {
    type Target = Runner<Env>;

    fn deref(&self) -> &Self::Target {
        &self.runner
    }
}

impl<'a, Env: Send> DerefMut for TestGuard<'a, Env> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.runner
    }
}

impl<'a, Env> Drop for TestGuard<'a, Env> {
    fn drop(&mut self) {
        let conn = self.runner.connection_pool().get().unwrap();
        ::diesel::sql_query("TRUNCATE TABLE background_jobs")
            .execute(&conn)
            .unwrap_from_drop();
    }
}
