import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {
  readApproverName,
  readHireHandoffPayGroups,
  readHireHandoffPeople,
  readRosterShift,
  type Lane8Actor,
} from "./native-lane-8/client";

/**
 * Owner-native app-entrypoint reads. The four former inline/fragment SQL
 * statements were removed: the axum owner executes them with sqlx, owns the
 * Company scope and projects the response. This module only transports bounded
 * identities and keeps the caller-visible result shapes unchanged.
 *
 * Callers that hold a WorkContext can pass it to select the exact registered
 * Holding. Legacy fact callers keep the existing signature and are resolved by
 * the exact incoming Host; there is no first-Holding fallback.
 */
export function hireHandoffPayGroups(db: SqlDatabase, companyId: string, actor?: Lane8Actor) {
  return readHireHandoffPayGroups(db, companyId, actor);
}

export function hireHandoffPeople(db: SqlDatabase, companyId: string, actor?: Lane8Actor) {
  return readHireHandoffPeople(db, companyId, actor);
}

export function requestApproverLegalName(
  db: SqlDatabase,
  companyId: string,
  userId: string,
  actor?: Lane8Actor,
) {
  return readApproverName(db, companyId, userId, actor);
}

export function overtimeRosterShift(
  db: SqlDatabase,
  shiftId: string,
  companyId: string,
  engagementId: string,
  actor?: Lane8Actor,
) {
  return readRosterShift(db, shiftId, companyId, engagementId, actor);
}
