import "server-only";
import { holdingResources } from "../../../../../../platform/runtime/server/src/public/runtime";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import { pmsWorkspace } from "../../public/pms";
import { pmsHome, pmsHotelSources } from "@erpx/pms-server/public/pms/home";
import {postAction,postComments,postView,postDraftSchema,publishPost,savePostDraft} from '@erpx/collaboration-server/public/social-home';
import { acknowledgeHandover,acknowledgeHandoverVersion, handoverAcknowledgement, publishHandover } from "@erpx/operations-server/public/handover";
import { textSuggestions, mentionsSchema } from "@erpx/collaboration-server/public/social-text";
import {readablePost} from '@erpx/collaboration-server/public/social-source';
import { createPmsHomeApi, type PmsHomeApiRepository } from "@erpx/pms-server/application/home-api";
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
