import "server-only";
import { holdingResources } from "../../../../../packages/server-runtime/src/public/runtime";
import { ApiError } from "@erpx/server-runtime/public/http";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { pmsWorkspace } from "../../../../../packages/server-pms/src/public/pms";
import { pmsHome, pmsHotelSources } from "@erpx/server-pms/public/pms/home";
import { postAction, postComments, postView, postDraftSchema, publishPost, savePostDraft } from "@erpx/server-collaboration/public/social-home";
import { acknowledgeHandover,acknowledgeHandoverVersion, handoverAcknowledgement, publishHandover } from "@erpx/server-operations/public/handover";
import { textSuggestions, mentionsSchema } from "@erpx/server-collaboration/public/social-text";
import { readablePost } from "@erpx/server-collaboration/public/social-source";
import { createPmsHomeApi, type PmsHomeApiRepository } from "@erpx/server-pms/application/home-api";
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
            acknowledgeHandoverVersion: (...args) => acknowledgeHandoverVersion(write, ...args),
            postAction: (...args) => postAction(write, ...args)
        };
        return repository;
    } });
export const { pmsHomeRead, pmsHomeWrite } = application;
