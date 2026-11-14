-- One-time route replacement; business node IDs and historical evidence remain intact.
LOCK TABLE organization_nodes IN ACCESS EXCLUSIVE MODE;
CREATE TABLE IF NOT EXISTS organization_route_replacements (
  node_id text PRIMARY KEY REFERENCES organization_nodes(id),
  old_slug text NOT NULL,
  new_slug text NOT NULL,
  replaced_at bigint NOT NULL
);
DROP TRIGGER organization_route_immutable ON organization_nodes;
DO $$
DECLARE node record; base text; replacement text; suffix integer;
BEGIN
  FOR node IN SELECT id,kind,slug,parent_id,branch_id FROM organization_nodes
    WHERE kind IN ('branch','department') AND slug IN ('apps','pos','pms','finance','people','sales','supply','assets','projects','recruit','learn','support','service','legal','manufacturing','admin','booking','portal','legal-portal','forms','book','customer-portal','departments','sso','api','_next','__erpx','login','logout','launch','choose-work-role','applications','account','provider','platform','robots.txt','manifest.webmanifest','brand','fonts','vendor') ORDER BY id
  LOOP
    base := left(node.slug,60) || CASE WHEN node.kind='branch' THEN '-branch' ELSE '-dept' END;
    replacement := base; suffix := 1;
    WHILE EXISTS(SELECT 1 FROM organization_nodes n WHERE n.slug=replacement AND n.kind=node.kind
      AND (node.kind='branch' OR n.parent_id IS NOT DISTINCT FROM node.parent_id OR n.branch_id IS NULL AND node.branch_id IS NULL))
    LOOP suffix := suffix+1; replacement := base || '-' || suffix; END LOOP;
    INSERT INTO organization_route_replacements(node_id,old_slug,new_slug,replaced_at)
      VALUES(node.id,node.slug,replacement,floor(extract(epoch FROM clock_timestamp())*1000)::bigint);
    UPDATE organization_nodes SET slug=replacement,hierarchy_revision=hierarchy_revision+1 WHERE id=node.id;
  END LOOP;
END $$;
CREATE TRIGGER organization_route_immutable BEFORE UPDATE OF slug,parent_id,kind ON organization_nodes
  FOR EACH ROW EXECUTE FUNCTION organization_route_immutable_fn();
