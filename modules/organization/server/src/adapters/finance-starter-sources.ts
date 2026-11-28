import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {nativeOrgAll} from './native-organization-owner-write';

/** The Company-owning read stays a row lock; only the node list is native-owned. */
export async function lockFinanceStarterCompany(db:SqlDatabase,companyId:string){return db.prepare("SELECT id FROM organization_nodes WHERE id=? AND kind='company' AND status='active' FOR UPDATE").bind(companyId).first();}

/** Native-owned branch/department list. The authoritative SQL and its single
 * Company bind live in the Organization owner crate; no TS statement remains. */
export async function financeStarterOrganizationNodes(db:SqlDatabase,companyId:string){return nativeOrgAll<{id:string;kind:string;display_name:string}>(db,companyId,'finance_starter_organization_nodes',[companyId]);}
