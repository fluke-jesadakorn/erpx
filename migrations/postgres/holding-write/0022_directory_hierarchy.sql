-- Last complete topology, atomically replaced with the identity/group snapshot.
-- Absence means no hierarchy has been synchronized yet; no reseed is needed.
CREATE TABLE iam_directory_hierarchies (
  connection_id TEXT PRIMARY KEY REFERENCES iam_connections(id) ON DELETE CASCADE,
  hierarchy_json JSONB NOT NULL DEFAULT '{"containers":[],"placements":[],"groupEdges":[]}'::jsonb,
  observed_at BIGINT NOT NULL
);
