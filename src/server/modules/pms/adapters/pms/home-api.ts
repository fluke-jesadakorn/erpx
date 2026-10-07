import "server-only";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { ApiError } from "@/server/modules/runtime/public/http";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { pmsWorkspace } from "@/server/modules/pms/public/pms";
import { pmsHome, pmsHotelSources } from "@/server/modules/pms/public/pms/home";
import { postAction, postComments, postView, postDraftSchema, publishPost, savePostDraft } from "@/server/modules/collaboration/public/social-home";
import { acknowledgeHandover, handoverAcknowledgement, publishHandover } from "@/server/modules/operations/public/handover";
import { textSuggestions, mentionsSchema } from "@/server/modules/collaboration/public/social-text";
import { readablePost } from "@/server/modules/collaboration/public/social-source";
import { createPmsHomeApi, type PmsHomeApiRepository } from "../../application/home-api";
const application = createPmsHomeApi({ require: requirePermission, postDraft: raw => postDraftSchema.parse(raw), mentions: raw => mentionsSchema.parse(raw), error: (status, code, message) => new ApiError(status, code, message), pmsHotelSources: (...args) => pmsHotelSources(...args), pmsWorkspace: (...args) => pmsWorkspace(...args), pmsHome: (...args) => pmsHome(...args), repository: async (actor) => {
        const { write } = await holdingResources(actor.holdingId);
        const repository: PmsHomeApiRepository = {
            readablePost: (...args) => readablePost(write, ...args),
            postView: (...args) => postView(write, ...args),
            postComments: (...args) => postComments(write, ...args),
            handoverAcknowledgement: (...args) => handoverAcknowledgement(write, ...args),
            textSuggestions: (...args) => textSuggestions(write, ...args),
            savePostDraft: (...args) => savePostDraft(write, ...args),
            publishPost: (...args) => publishPost(write, ...args),
            publishHandover: (...args) => publishHandover(write, ...args),
            acknowledgeHandover: (...args) => acknowledgeHandover(write, ...args),
            postAction: (...args) => postAction(write, ...args)
        };
        return repository;
    } });
export const { pmsHomeRead, pmsHomeWrite } = application;
