import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import 'server-only';
import { ApiError } from "@/server/modules/runtime/public/http";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { readBoundedUploadBody } from "@/server/modules/files/public/attachments/upload";
import { attachmentContent, attachmentLinkStatus, bindRecordAttachmentSet, completeAttachmentUpload, createAttachmentSet, listAttachmentSets, reserveAttachmentFile, withdrawAttachmentLink } from "@/server/modules/files/public/attachments/service";
import { createFrontPmsAttachments, type FrontPmsAttachmentRepository } from "../../application/front-attachments";
import { pmsFrontAttachmentFacts } from "../../../files/public/pms-front-attachment-facts";
const application = createFrontPmsAttachments({ require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), origin: requestPublicOrigin, readBody: readBoundedUploadBody, reply: (data, status) => Response.json(canonicalDestinationLinks({ data }), { status, headers: { "Cache-Control": "private, no-store" } }), content: result => { const headers = new Headers({ 'Content-Type': result.mediaType, 'Content-Disposition': `${result.inline ? 'inline' : 'attachment'}; filename*=UTF-8''${encodeURIComponent(result.inline ? `preview-${result.filename}` : result.filename)}`, 'Cache-Control': 'private, no-store', 'X-Content-Type-Options': 'nosniff', 'Accept-Ranges': 'none' }); if (result.inline)
        headers.set('Content-Security-Policy', "sandbox; default-src 'none'"); return new Response(result.object.body, { headers }); }, repository: async (actor) => {
        const { write, files } = await holdingResources(actor.holdingId);
        const repository: FrontPmsAttachmentRepository = { ...pmsFrontAttachmentFacts(write),
            attachmentContent: (...args) => attachmentContent(write, files, ...args),
            attachmentLinkStatus: (...args) => attachmentLinkStatus(write, ...args),
            bindRecordAttachmentSet: (...args) => bindRecordAttachmentSet(write, ...args),
            completeAttachmentUpload: (...args) => completeAttachmentUpload(write, files, ...args),
            createAttachmentSet: (...args) => createAttachmentSet(write, ...args),
            listAttachmentSets: (...args) => listAttachmentSets(write, ...args),
            reserveAttachmentFile: (...args) => reserveAttachmentFile(write, ...args),
            withdrawAttachmentLink: (...args) => withdrawAttachmentLink(write, ...args) };
        return repository;
    } });
export const { frontPmsAttachmentRequest, frontPmsAttachmentPaths } = application;
