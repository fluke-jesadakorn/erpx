import {ApiError} from "../../../runtime/public/http";
import {createBoundedAttachmentUpload} from "../../application/attachments/upload";
export const ATTACHMENT_UPLOAD_MAX_BYTES=10*1024*1024;
export const {readBoundedUploadBody}=createBoundedAttachmentUpload((status,code,message)=>new ApiError(status,code,message));
