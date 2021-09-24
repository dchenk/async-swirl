use diesel::prelude::*;
use std::error::Error;
use std::time::Instant;
use swirl::*;

#[swirl::background_job]
fn dummy_job() -> Result<(), PerformError> {
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let database_url = dotenv::var("DATABASE_URL")?;
    println!("Enqueuing 100k jobs");

    let pool_manager =
        deadpool_diesel::postgres::Manager::new(&database_url, deadpool_diesel::Runtime::Tokio1);
    let db_pool = deadpool_diesel::postgres::Pool::builder(pool_manager)
        .max_size(5)
        .build()
        .into(Into::into)?;

    let runner = Runner::builder((), db_pool.clone()).build();
    let mut conn: PgConnection = db_pool.get().await.unwrap().lock().unwrap().deref_mut();
    enqueue_jobs(&mut conn).unwrap();
    println!("Running jobs");
    let started = Instant::now();

    runner.run_all_pending_jobs().await?;
    runner.check_for_failed_jobs()?;

    let elapsed = started.elapsed();
    println!("Ran 100k jobs in {} seconds", elapsed.as_secs());

    Ok(())
}

fn enqueue_jobs(conn: &mut PgConnection) -> Result<(), EnqueueError> {
    use diesel::sql_query;
    sql_query("TRUNCATE TABLE background_jobs;").execute(conn)?;
    for _ in 0..100_000 {
        dummy_job().enqueue(conn)?;
    }
    Ok(())
}
