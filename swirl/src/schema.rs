table! {
    background_jobs (id) {
        id -> Text,
        job_type -> Text,
        data -> Jsonb,
        retries -> Int4,
        last_retry_at -> Nullable<Timestamp>,
        created_at -> Timestamp,
    }
}
