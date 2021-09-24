// use async_trait::async_trait;
// use diesel::PgConnection;
// use std::error::Error;
// use std::ops::DerefMut;

/*
/// A trait to work around associated type constructors.
///
/// This will eventually change to `type Connection<'a>` on [`DieselPool`]
pub trait BorrowedConnection<'a> {
    /// The smart pointer returned by this connection pool.
    type Connection: DerefMut<Target = PgConnection>;
}


// pub type DieselPooledConn<'a, T> = <T as BorrowedConnection<'a>>::Connection;

/// A connection pool for Diesel database connections.
// pub trait DieselPool: Clone + Send + for<'a> BorrowedConnection<'a> {
// #[async_trait]
// pub trait DieselPool: Clone + Send {
//     /// The error type returned when a connection could not be retrieved from
//     /// the pool.
//     type Error: Error + Send + Sync + 'static;
//
//     /// Attempt to get a database connection from the pool. Errors if a
//     /// connection could not be retrieved from the pool.
//     ///
//     /// The exact details of why an error would be returned will depend on
//     /// the pool, but a reasonable implementation will return an error if:
//     /// - A timeout was reached
//     /// - An error occurred establishing a new connection
//     async fn get(&self) -> Result<dyn DerefMut<Target = PgConnection>, Self::Error>;
//     // async fn get(&self) -> Result<dyn DerefMut<Target = Self>, Self::Error>;
//     // async fn get(&self) -> Result<DieselPooledConn<'_, Self>, Self::Error>;
// }

// /// Object safe version of [`DieselPool`]
// pub trait DieselPoolObj {
//     type C: DerefMut<Target = PgConnection>;
//     type Error;
//
//     /// Object safe version of [`DieselPool::get`]
//     ///
//     /// This function will heap allocate the connection. This allocation can
//     /// be avoided by using [`Self::with_connection`]
//     fn get(&self) -> Result<Self::C, Self::Error>;
//     // fn get(&self) -> Result<Box<dyn DerefMut<Target = PgConnection> + '_>, Box<dyn Error>>;
//
//     fn with_connection(
//         &self,
//         f: &dyn Fn(&mut PgConnection) -> Result<(), Box<dyn Error>>,
//     ) -> Result<(), Box<dyn Error>>;
// }


impl<T: DieselPool> DieselPoolObj for T {
    fn get(&self) -> Result<Box<dyn DerefMut<Target = PgConnection> + '_>, Box<dyn Error>> {
        DieselPool::get(self).map(|v| Box::new(v) as _).map_err(|v| Box::new(v) as _)
    }

    fn with_connection(
        &self,
        f: &dyn Fn(&mut PgConnection) -> Result<(), Box<dyn Error>>,
    ) -> Result<(), Box<dyn Error>> {
        let conn = DieselPool::get(self)?;
        f(conn)
    }
}

// impl<'a> BorrowedConnection<'a> for deadpool_diesel::postgres::Pool {
//     type Connection = deadpool_diesel::postgres::Connection;
// }

#[async_trait]
impl DieselPool for deadpool_diesel::postgres::Pool {
    type Error = deadpool_diesel::PoolError;

    async fn get(&self) -> Result<deadpool_diesel::postgres::Connection, Self::Error> {
        // async fn get<'a>(&'a self) -> Result<DieselPooledConn<'a, Self>, Self::Error> {
        self.get().await
    }
}


mod db_pool_impl {
    use super::*;

    type ConnectionManager = deadpool_diesel::postgres::Manager;

    impl<'a> BorrowedConnection<'a> for deadpool_diesel::postgres::Pool {
        type Connection = deadpool_diesel::postgres::Connection;
    }

    impl DieselPool for deadpool_diesel::postgres::Pool {
        type Error = deadpool_diesel::PoolError;

        fn get<'a>(&'a self) -> Result<DieselPooledConn<'a, Self>, Self::Error> {
            self.get()
        }
    }

    pub struct DBPoolBuilder {
        url: String,
        builder: deadpool_diesel::managed::PoolBuilder<ConnectionManager>, // TODO
                                                                           // connection_count: Option<usize>,
    }

    impl DBPoolBuilder {
        pub(crate) fn new(
            url: String,
            builder: deadpool_diesel::managed::PoolBuilder<ConnectionManager>,
        ) -> Self {
            Self {
                url,
                builder,
                // connection_count: None,
            }
        }

        // pub(crate) fn connection_count(&mut self, connection_count: usize) {
        //     self.connection_count = Some(connection_count);
        // }

        /*
        pub(crate) fn build(
            self,
            default_connection_count: usize,
        ) -> Result<
            deadpool_diesel::postgres::Pool,
            deadpool_diesel::managed::BuildError<deadpool_diesel::postgres::Manager::Error>,
        > {
            let max_size = self.connection_count.unwrap_or(default_connection_count);
            self.builder.max_size(max_size).build()
            // .build(ConnectionManager::new(self.url))
        }
         */
    }
}

#[doc(hidden)]
pub use self::db_pool_impl::DBPoolBuilder;
*/
