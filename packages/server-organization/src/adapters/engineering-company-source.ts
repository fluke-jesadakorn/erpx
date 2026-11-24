import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {companyNodeByIdKind} from './organization-column-alias-families-r31';
export async function engineeringCompanySource(db:SqlDatabase,companyId:string):Promise<boolean>{return !!await db.prepare(companyNodeByIdKind('1')).bind(companyId).first();}
