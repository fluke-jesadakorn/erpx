import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ORG_COMPANY_DISPLAY_NAME_SOURCE} from './org-sources';
export function companySource(database:SqlDatabase){return {company(companyId:string){return database.prepare(ORG_COMPANY_DISPLAY_NAME_SOURCE).bind(companyId).first<{display_name:string}>();}};}
