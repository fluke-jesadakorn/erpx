import {ApiError} from "@/server/modules/runtime/public/http";
import {forbidden} from "next/navigation";
import {createRequirePermission} from "../application/permissions";
export const requirePermission=createRequirePermission({forbidden,error:(status,code,message)=>new ApiError(status,code,message)});
