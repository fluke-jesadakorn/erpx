import {ApiError} from "@erpx/server-runtime/public/http";
import {forbidden} from "next/navigation";
import {createRequirePermission} from "@erpx/server-identity_access/application/permissions";
export const requirePermission=createRequirePermission({forbidden,error:(status,code,message)=>new ApiError(status,code,message)});
