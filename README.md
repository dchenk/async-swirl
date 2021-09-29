Swirl
=====

A simple, efficient background work queue for Rust
--------------------------------------------------

Swirl is a background work queue built on Diesel and PostgreSQL's row locking
features. It was extracted from [crates.io](crates.io), which uses it for
updating the index off the web server.

This library is still in its early stages, and has not yet reached 1.0 status.

## Getting Started

Swirl stores background jobs in your PostgreSQL 9.5+ database. As such, it has
migrations which need to be run. At the moment, this should be done by copying
our `migrations` directory into your own.

Jobs in Swirl are defined as functions annotated with
`#[swirl::background_job]`, like so:

```rust
#[swirl::background_job]
fn resize_image(file_name: String, dimensions: Size) -> Result<(), swirl::PerformError> {
    // Do expensive computation that shouldn't be done on the web server
}
```

All arguments must implement `serde::Serialize` and `serde::DeserializeOwned`.
Jobs can also take a shared "environment" argument. This is a struct you define,
which can contain resources shared between jobs like a connection pool, or
application level configuration. For example:

```rust
struct Environment {
    file_server_private_key: String,
    http_client: http_lib::Client,
}

#[swirl::background_job]
fn resize_image(
    env: &Environment,
    file_name: String,
    dimensions: Size,
) -> Result<(), swirl::PerformError> {
    // Do expensive computation that shouldn't be done on the web server
}
```

Note that all jobs must use the same type for the environment.
Once a job is defined, it can be enqueued like so:

```rust
resize_image(file_name, dimensions).enqueue(&diesel_connection)?
```

You do not pass the environment when enqueuing jobs.
Jobs are run asynchronously by an instance of `swirl::Runner`. To construct
one, you must first pass it the job environment (this is `()` if your jobs don't
take an environment), and a Diesel connection pool (from [`deadpool-diesel`](https://github.com/dchenk/deadpool)).

```rust
let runner = Runner::builder(environment, connection_pool)
    .build();
```

At the time of writing, it is up to you to make sure your connection pool is
well configured for your runner. Your connection pool size should be at least as
big as the thread pool size (defaults to the number of CPUs on your machine), or
double that if your jobs require a database connection.

Once the runner is created, calling `start` will continuously saturate the number
of specified worker threads, running one job per thread at a time. The function
never returns (TODO: catch a CTRL+C signal) Ok but returns an Err if an error
processing jobs occurs or until a job panics. This function does not stop when a
normal error occurs within the execution of a job.

When a job fails (by returning an error or panicking), it will be retried after
`1 ^ {retry_count}` minutes. If a job fails or an error occurs marking a job as
finished/failed, it will be logged to stderr. No output will be sent when jobs
are running successfully.

Swirl uses at least once semantics. This means that we guarantee all jobs are
successfully run to completion, but we do not guarantee that it will do so only
once, even if the job successfully returns `Ok(())`. Therefore, it is important
that all jobs are idempotent.

## Upcoming features

Planned features that are not yet implemented are:

- More robust and configurable logging
- Configurable retry behavior
- Less boilerplate in the job runner

## Code of conduct

Anyone who interacts with Swirl in any space, including but not limited to
this GitHub repository, must follow our [code of conduct](https://github.com/dchenk/swirl/blob/master/code_of_conduct.md).

## License

Licensed under either of these:

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   https://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   https://opensource.org/licenses/MIT)

### Contributing

Unless you explicitly state otherwise, any contribution you intentionally submit
for inclusion in the work, as defined in the Apache-2.0 license, shall be
dual-licensed as above, without any additional terms or conditions.
