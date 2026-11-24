import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {assetScopeTupleSql} from "./asset-scope-tuple";
import "server-only";
/** Canonical owner-local fixed SQL for an Assets current-assignment permission
 * probe. Every purchase and metric caller shares this one literal so the TS
 * read tail carries no duplicate statement. The scope tuple is the byte-identical
 * owner-local `assetScopeTupleSql` literal; callers pass only a fixed scope alias
 * and permission identifier; no SQL or relation name is accepted. */
export const assetPermissionProbeSql = (alias: string, permission: string) =>
  `SELECT ${assignmentReadSql(alias, permission)} allowed FROM ${assetScopeTupleSql(alias)}`;
