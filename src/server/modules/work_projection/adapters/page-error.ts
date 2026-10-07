import {headers} from "next/headers";
import {forbidden,notFound,redirect,unstable_rethrow} from "next/navigation";
import {requestedHoldingScope} from "../../identity_access/public/authorization";
import {ApiError} from "../../runtime/public/http";
import {createPageErrorService} from "../application/page-error";
export const {currentPagePath,accessPage,pageError}=createPageErrorService({headers,requestedHoldingScope,forbidden,notFound,redirect,unstableRethrow:unstable_rethrow,isApiError:(error):error is ApiError=>error instanceof ApiError});
