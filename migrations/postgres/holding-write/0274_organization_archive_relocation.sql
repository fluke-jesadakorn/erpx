-- Organization changes keep archived units discoverable for history while
-- excluding them from active authorization and workflow placement.
ALTER TABLE organization_nodes DROP CONSTRAINT organization_nodes_status_check;
ALTER TABLE organization_nodes ADD CONSTRAINT organization_nodes_status_check
  CHECK(status IN ('active','suspended','archived'));

ALTER TABLE organization_node_changes DROP CONSTRAINT organization_node_changes_action_check;
ALTER TABLE organization_node_changes ADD CONSTRAINT organization_node_changes_action_check
  CHECK(action IN ('create','rename','suspend','reactivate','archive','relocate'));

ALTER TABLE organization_node_changes ADD COLUMN source_branch_id TEXT;
CREATE INDEX organization_node_change_source_branch
  ON organization_node_changes(company_id,source_branch_id,state);
