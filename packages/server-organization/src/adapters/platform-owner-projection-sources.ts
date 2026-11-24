import "server-only";
import {ORG_NODES_SOURCE} from './org-sources';
/** Closed owner projections for Platform, Organization and Procurement joins. Original filters, bindings and supplied transactions remain with the caller. */
export const platformOwnerProjectionSources = Object.freeze({
  company_business_acceptance: "SELECT \"company_id\",\"status\",\"version\" FROM company_business_acceptance",
  organization_nodes: ORG_NODES_SOURCE,
} as const);
