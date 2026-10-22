import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {ExecutiveConnectionGrant} from "@erpx/server-runtime/public/executive/connections";
export function executiveConnectionIdentityAccessFacts(db:SqlDatabase){return {activeGrant:(facts:{value1:string;value2:string;value3:string;value4:string;value5:number;value6:"holding" | "branch" | "department";value7:string | null;value8:number;value9:number;value10:number;value11:number;})=>db.prepare(`SELECT a.id,a.role,a.version,a.scope_kind,a.scope_node_id,a.branch_id,a.modules_json,a.valid_until
    FROM assignments a ${organizationSources.executive_connection_facts_organization_nodes_c_join} ON c.id=a.company_id AND c.status='active' AND c.kind='company'
    WHERE a.id=? AND a.company_id=? AND a.user_id=? AND a.role=? AND a.version=? AND a.scope_kind=?
      AND a.scope_node_id IS NOT DISTINCT FROM ?::text AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>?
      AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
      AND EXISTS(SELECT 1 FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
        WHERE r.id=a.role AND r.company_id IN ('*',a.company_id) AND r.status='active' AND v.scopes_json @> jsonb_build_array(a.scope_kind))`).bind(facts.value1,facts.value2,facts.value3,facts.value4,facts.value5,facts.value6,facts.value7,facts.value8,facts.value9,facts.value10,facts.value11).first<ExecutiveConnectionGrant>(),
selectedGrant:(facts:{value1:string;value2:string;value3:string;})=>db.prepare("SELECT id,role,version,scope_kind,scope_node_id,branch_id,modules_json,valid_until FROM assignments WHERE id=? AND company_id=? AND user_id=?").bind(facts.value1,facts.value2,facts.value3).first<ExecutiveConnectionGrant>()};}
