pub fn build_pool() -> swirl::DieselPool {
    let database_url =
        dotenv::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL must be set to run tests");

    let pool_manager =
        deadpool_diesel::postgres::Manager::new(&database_url, deadpool_diesel::Runtime::Tokio1);

    swirl::DieselPool::builder(pool_manager).max_size(5).build().unwrap()
}
