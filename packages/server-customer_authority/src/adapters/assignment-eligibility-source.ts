import 'server-only';
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";

/** Byte-identical assignment-projection existence core shared by the customer
 * access actor/reviewer eligibility read and the holding-scoped manager
 * eligibility read. The caller owns the alias and predicate text so each
 * reader keeps its exact verified SQL while one shared core cannot drift. */
export const assignmentEligibilityExists = (body: string) => `EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) ${body})`;
