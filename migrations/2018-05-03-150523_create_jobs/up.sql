CREATE TABLE background_jobs (
  id TEXT NOT NULL PRIMARY KEY,
  job_type TEXT NOT NULL,
  data JSONB NOT NULL,
  status TEXT NOT NULL,
  retries INTEGER NOT NULL DEFAULT 0,
  last_retry_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now()
);

create index background_jobs_created_at_filtered_idx on background_jobs2 (created_at)
where status IN ('Queued', 'FailedReQueued');
