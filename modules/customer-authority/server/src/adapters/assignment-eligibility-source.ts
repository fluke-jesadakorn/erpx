import 'server-only';
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/identity-access-server/public/final-root-owner-projection-sources";

/** One byte-identical assignment-projection source shared by the customer
 * access actor/reviewer eligibility read, the distinct reviewers COUNT and the
 * holding-scoped manager eligibility read. The caller owns the projection and
 * alias/predicate text so each reader keeps its exact verified SQL while one
 * shared core cannot drift. */
export const assignmentSource = (projection = '1') => `SELECT ${projection} FROM (${identity_accessProjectionSources.assignments})`;

/** Byte-identical assignment-projection existence core; caller owns alias and
 * predicate text. */
export const assignmentEligibilityExists = (body: string) => `EXISTS(${assignmentSource()} ${body})`;
