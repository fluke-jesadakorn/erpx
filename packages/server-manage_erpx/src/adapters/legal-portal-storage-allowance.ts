import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {betaPolicyStorageBytesSql} from "./attachments-service-facts";
export function legalPortalStorageAllowance(db:SqlDatabase,companyId:string){return db.prepare(betaPolicyStorageBytesSql).bind(companyId).first<{storage_bytes:number}>();}
