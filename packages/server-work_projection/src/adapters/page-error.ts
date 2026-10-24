import {headers} from "next/headers";
import {forbidden,notFound,redirect,unstable_rethrow} from "next/navigation";
import {requestedHoldingScope} from "@erpx/server-identity_access/public/requested-holding-scope";
import {ApiError} from "@erpx/server-runtime/public/http";
import {createPageErrorService} from "@erpx/server-work_projection/application/page-error";
export const {currentPagePath,accessPage,pageError}=createPageErrorService({headers,requestedHoldingScope,forbidden,notFound,redirect,unstableRethrow:unstable_rethrow,isApiError:(error):error is ApiError=>error instanceof ApiError});
