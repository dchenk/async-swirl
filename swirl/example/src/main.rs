use diesel::prelude::*;
use std::error::Error;
use std::time::Instant;
use swirl::*;

#[swirl::background_job]
fn dummy_job() -> Result<(), PerformError> {
    println!("dummy_job");
    Ok(())
}

#[swirl::background_job]
async fn dummy_job2(_db_pool: swirl::DieselPool, x: String, y: String) -> Result<(), PerformError> {
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    println!("dummy_job2: {}, {} - {:?}", x, y, std::time::SystemTime::now());
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let database_url = dotenv::var("DATABASE_URL")?;

    println!("Enqueuing 100k jobs");
    println!("Time: {:?}", std::time::SystemTime::now());

    let pool_manager =
        deadpool_diesel::postgres::Manager::new(&database_url, deadpool_diesel::Runtime::Tokio1);
    let db_pool = deadpool_diesel::postgres::Pool::builder(pool_manager).max_size(5).build()?;

    let runner = Runner::builder((), db_pool.clone())
        .concurrency(3)
        .job_timeout(std::time::Duration::from_secs(2))
        .build();

    db_pool
        .get()
        .await
        .unwrap()
        .interact(|conn| {
            enqueue_jobs(&mut *conn).unwrap();
            Ok(())
        })
        .await
        .unwrap();

    println!("Running jobs");
    let started = Instant::now();

    runner.start().await?;

    println!("Jobs finished");

    let elapsed = started.elapsed();
    println!("Ran 100k jobs in {} seconds", elapsed.as_secs());

    Ok(())
}

fn enqueue_jobs(conn: &mut PgConnection) -> Result<(), EnqueueError> {
    use diesel::sql_query;
    sql_query("TRUNCATE TABLE background_jobs;").execute(conn)?;
    for i in 0..100_000 {
        dummy_job().enqueue(conn)?;
        dummy_job2(format!("{}", i), format!("{}", i + 1)).enqueue(conn)?;
    }
    Ok(())
}
