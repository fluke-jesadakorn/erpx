import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ownerProjection} from './organization-column-alias-families-r35';
/** Original Company placement identity, including retained inactive nodes. */
export const talentSettlementPlacements=ownerProjection('id,company_id,branch_id,kind,status','organization_nodes');
export function talentSettlementPlacementReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE organization_nodes IN SHARE MODE")];}
