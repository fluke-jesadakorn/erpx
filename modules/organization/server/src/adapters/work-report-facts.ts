import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {nativeOrgAll} from './native-organization-owner-write';

/** Native-owned work-report node facts. The authoritative SQL and Company
 * scope live in the Organization owner crate; this adapter sends only the
 * typed kind and keeps the retained `{results}` consumer shape. */
export function workReportOrganizationFacts(db:SqlDatabase){return {nodes:(companyId:string)=>nativeOrgAll<{id:string;display_name:string}>(db,companyId,'work_report_organization_nodes',[companyId]).then(results=>({results}))};}
