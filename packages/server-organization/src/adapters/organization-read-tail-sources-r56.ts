/** Retired organization-node absence guard head (r56).
 * Family: the beta empty-bootstrap forest and the two relocation assertions.
 * The body after `organization_nodes` is supplied verbatim by each caller;
 * bind arrays stay at the call sites. */
export const notExistsOrgNodeSql = (body: string) =>
  `NOT EXISTS(SELECT 1 FROM organization_nodes${body})`;
