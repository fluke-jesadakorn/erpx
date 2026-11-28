import {headers} from "next/headers";
import {forbidden,notFound,redirect,unstable_rethrow} from "next/navigation";
import {requestedHoldingScope} from "@erpx/identity-access-server/public/requested-holding-scope";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {createPageErrorService} from "@erpx/work-projection-server/application/page-error";
export const {currentPagePath,accessPage,pageError}=createPageErrorService({headers,requestedHoldingScope,forbidden,notFound,redirect,unstableRethrow:unstable_rethrow,isApiError:(error):error is ApiError=>error instanceof ApiError});
