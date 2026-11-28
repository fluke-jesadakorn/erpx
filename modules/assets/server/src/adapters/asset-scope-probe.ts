import {assignmentReadSql} from "@erpx/identity-access-server/public/record-source-authority";
import {assetProjectionFromSql,assetScopeTupleSql} from "./asset-scope-tuple";
import "server-only";
/** Canonical owner-local fixed SQL for an Assets current-assignment permission
 * probe. Every purchase and metric caller shares the parameterized projection-from
 * head, so the TS read tail carries no local statement token. The assignment-read
 * projection and scope tuple stay byte-identical; callers pass only a fixed scope
 * alias and permission identifier; no SQL or relation name is accepted. */
export const assetPermissionProbeSql = (alias: string, permission: string) =>
  assetProjectionFromSql(`${assignmentReadSql(alias, permission)} allowed`, ` ${assetScopeTupleSql(alias)}`);
