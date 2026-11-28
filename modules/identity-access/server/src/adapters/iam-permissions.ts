import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {forbidden} from "next/navigation";
import {createRequirePermission} from "@erpx/identity-access-server/application/permissions";
export const requirePermission=createRequirePermission({forbidden,error:(status,code,message)=>new ApiError(status,code,message)});
