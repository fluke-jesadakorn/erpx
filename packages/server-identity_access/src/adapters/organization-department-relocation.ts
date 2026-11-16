import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function stageOrganizationDepartmentAssignment(db:SqlDatabase,companyId:string,sourceId:string|null,branchId:string|null,nodeId:string){return db.prepare("SELECT 1").bind(); }
