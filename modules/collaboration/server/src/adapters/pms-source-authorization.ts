import { pmsSourcesSql } from "@erpx/pms-server/public/pms/source-query";
import { pmsSourceReadableSql } from "@erpx/identity-access-server/public/pms-source-authorization";
import { socialPostSourcePmsGuard } from './read-tail-sql';
/** PMS post text itself is private, not just the link preview. */
export function pmsPostSourcesReadableSql(post: string, user: string, assignment?: string) {
    return socialPostSourcePmsGuard(post, `
    AND NOT EXISTS(SELECT 1 FROM (${pmsSourcesSql}) pr WHERE pr.id=ps.source_id
      AND pr.company_id=${post}.company_id AND ${pmsSourceReadableSql("pr", user, assignment)})`);
}
