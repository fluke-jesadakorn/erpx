import "server-only";
import {ownerProjection} from './organization-column-alias-families-r35';
/** Closed owner read projections; consumers retain original filters, aliases and binds in their supplied statement/transaction. */
export const lastAuditOwnerProjectionSources = {
  spending_approval_templates: ownerProjection('"company_id","version","exceptions_json","reason","updated_by","updated_at"','spending_approval_templates'),
} as const;
