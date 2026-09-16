ALTER TABLE messaging_teams ADD COLUMN organization_managed BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE messaging_rooms ADD COLUMN organization_node_id TEXT REFERENCES organization_nodes(id);
CREATE UNIQUE INDEX messaging_organization_channel ON messaging_rooms(company_id,organization_node_id) WHERE organization_node_id IS NOT NULL;
ALTER TABLE messaging_rooms ADD CONSTRAINT organization_channel_shape CHECK(organization_node_id IS NULL OR (kind='channel' AND private=false));
