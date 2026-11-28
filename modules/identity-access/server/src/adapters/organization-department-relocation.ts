import { nativePrepare } from '@erpx/identity-access-server/public/native-identity-write';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
export function stageOrganizationDepartmentAssignment(db:SqlDatabase,companyId:string,sourceId:string|null,branchId:string|null,nodeId:string){return nativePrepare("w_cc6759980e3f").bind(branchId,nodeId,companyId,sourceId); }
