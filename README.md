Swirl
=====

A simple, efficient background work queue for Rust
--------------------------------------------------

Swirl is a background work queue built on Diesel and PostgreSQL's row locking features. It is  a fork
of [github.com/sgrif/swirl](https://github.com/sgrif/swirl), which extracted this library from the
code that powers [crates.io](https://crates.io).

Unlike sgrif/swirl, this crate:
- Supports `async` job functions.
- Uses Tokio to run jobs, which means that we don't need an additional library for thread pooling.
- Supports only [deadpool](https://crates.io/crates/deadpool) (i.e., [deadpool-diesel](https://crates.io/crates/deadpool-diesel))
for database connection pooling. Although this means that dchenk/async-swirl has fewer customization options
in this regard, the result is _much simpler_ code and a streamlined API.
- Has far more conservative error handling. For example, we deliberately cease to start processing
new jobs when we cannot process a job or when a job panics. (This does _not_ mean that we
cease to start processing new jobs if a job _returns_ an error, in which case the job is re-queued
and we carry on.)
- Supports configuring the number of retries to attempt for jobs.
- Does not delete succeeded jobs from the database and instead keeps track of the status in a column.

This library is still in its early stages and has not yet reached 1.0 status.

## Getting Started

Swirl stores background jobs in your PostgreSQL 9.5+ database. As such, it has migrations that
need to be run. At the moment, this is done by copying our `migrations` directory into your own.

Jobs in Swirl are defined as functions annotated with `#[swirl::background_job]` like so:

```rust
#[swirl::background_job]
async fn resize_image(file_name: String, dimensions: Size) -> Result<(), swirl::PerformError> {
    // Do expensive computation that shouldn't be done on the web server.
}
```

All arguments must implement `serde::Serialize` and `serde::DeserializeOwned`. Jobs can also
take a shared "environment" argument. This is a struct you define, which can contain resources
shared between jobs, like a connection pool or application level configuration. For example:

```rust
pub struct Environment {
    file_server_private_key: String,
    http_client: http_lib::Client,
}

#[swirl::background_job]
async fn resize_image(
    env: &Environment,
    db_pool: swirl::DieselPool,
    file_name: String,
    dimensions: Size,
) -> Result<(), swirl::PerformError> {
    // Do expensive computation that shouldn't be done on the web server.
}
```

The environment parameter is taken by reference, and all other parameters are owned. All jobs must use
the same type for the environment.

The database connection parameter must have the type `swirl::DieselPool`, which is a type alias for the
pool type in `deadpool-diesel`.

Once a job is defined, it can be enqueued like so:

```rust
resize_image(file_name, dimensions).enqueue(&diesel_connection)?
```

You do not pass the environment or database connection pool when enqueuing jobs.

Jobs are run asynchronously by an instance of `swirl::Runner`. You construct a `Runner` by calling its
associated `builder` method with the required job environment (this is `()` if your jobs don't take an
environment) and a `deadpool-diesel` connection pool:

```rust
let runner = Runner::builder(environment, connection_pool)
    .build();
```

It is up to you to make sure your connection pool is well configured for your runner. Your connection
pool size should be at least as big as the thread pool size, or double that if your jobs require
a database connection.

Once the runner is created, calling `start` will continuously saturate the number of specified
worker threads, running one job per thread at a time. The function never returns an `Ok` but
returns an `Err` if an error processing jobs occurs or if a job panics. This function does not stop
when a normal error occurs within the execution of a job. (TODO: catch a CTRL+C signal.)

When a job fails (by returning an error or panicking), it will be retried after `1 ^ {retry_count}`
minutes. If a job fails or an error occurs marking a job as finished/failed, it will be logged to
stderr. No output will be sent when jobs are running successfully.

Swirl uses at least once semantics. This means that we guarantee all jobs are successfully run to
completion, but we do not guarantee that it will do so only once, even if the job successfully
returns `Ok(())`. Therefore, it is important that all jobs are idempotent.

## Upcoming features

Planned features that are not yet implemented are:

- More robust and configurable logging
- Configurable retry behavior

## Code of conduct

Anyone who interacts with Swirl in any space, including but not limited to
this GitHub repository, must follow our [code of conduct](https://github.com/dchenk/async-swirl/blob/master/code_of_conduct.md).

## License

Licensed under either of these:

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   https://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   https://opensource.org/licenses/MIT)

### Contributing

Unless you explicitly state otherwise, any contribution you intentionally submit for inclusion in
the work, as defined in the Apache-2.0 license, shall be dual-licensed as above, without any
additional terms or conditions.
