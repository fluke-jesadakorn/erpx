import { pmsSourcesSql } from "@erpx/server-pms/public/pms/source-query";
import { pmsSourceReadableSql } from "@erpx/server-identity_access/public/pms-source-authorization";
/** PMS post text itself is private, not just the link preview. */
export function pmsPostSourcesReadableSql(post: string, user: string, assignment?: string) {
    return `NOT EXISTS(SELECT 1 FROM social_post_sources ps WHERE ps.post_id=${post}.id AND ps.source_kind='pms_record'
    AND NOT EXISTS(SELECT 1 FROM (${pmsSourcesSql}) pr WHERE pr.id=ps.source_id
      AND pr.company_id=${post}.company_id AND ${pmsSourceReadableSql("pr", user, assignment)}))`;
}
