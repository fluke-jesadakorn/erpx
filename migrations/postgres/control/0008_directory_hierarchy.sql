CREATE TABLE iam_directory_hierarchies (
  connection_id TEXT PRIMARY KEY REFERENCES iam_connections(id) ON DELETE CASCADE,
  hierarchy_json JSONB NOT NULL DEFAULT '{"containers":[],"placements":[],"groupEdges":[]}'::jsonb,
  observed_at BIGINT NOT NULL
);
