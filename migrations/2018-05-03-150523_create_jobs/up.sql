CREATE TABLE background_jobs (
  id TEXT NOT NULL PRIMARY KEY,
  job_type TEXT NOT NULL,
  data JSONB NOT NULL,
  retries INTEGER NOT NULL DEFAULT 0,
  last_retry_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now()
);
