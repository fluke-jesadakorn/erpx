import { nativeOrganizationRelocation } from "@erpx/collaboration-server/adapters/native-organization-relocation";
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
export async function stageOrganizationDepartmentPeople(db:SqlDatabase,companyId:string,sourceId:string|null,branchId:string|null,nodeId:string,now:number){const native=nativeOrganizationRelocation(db);return [
 await native.engagement({companyId,branchId,nodeId,now,sourceId}),

          await native.employee({companyId,branchId,nodeId,sourceId})]; }
