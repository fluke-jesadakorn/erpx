import { z } from "zod";
import type { WorkContext } from "../../identity_access/public/authorization";
type DropDatabase<Fn> = Fn extends (database: infer _Database, ...args: infer Args) => infer Result ? (...args: Args) => Result : never;
export type PmsHomeApiRepository = {
    readablePost: DropDatabase<typeof import("../../collaboration/public/social-source").readablePost>;
    postView: DropDatabase<typeof import("../../collaboration/public/social-home").postView>;
    postComments: DropDatabase<typeof import("../../collaboration/public/social-home").postComments>;
    handoverAcknowledgement: DropDatabase<typeof import("../../operations/public/handover").handoverAcknowledgement>;
    textSuggestions: DropDatabase<typeof import("../../collaboration/public/social-text").textSuggestions>;
    savePostDraft: DropDatabase<typeof import("../../collaboration/public/social-home").savePostDraft>;
    publishPost: DropDatabase<typeof import("../../collaboration/public/social-home").publishPost>;
    publishHandover: DropDatabase<typeof import("../../operations/public/handover").publishHandover>;
    acknowledgeHandover: DropDatabase<typeof import("../../operations/public/handover").acknowledgeHandover>;
    acknowledgeHandoverVersion: DropDatabase<typeof import("../../operations/public/handover").acknowledgeHandoverVersion>;
    postAction: DropDatabase<typeof import("../../collaboration/public/social-home").postAction>;
};
export function createPmsHomeApi(ports: {
    repository(actor: WorkContext): Promise<PmsHomeApiRepository>;
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
    postDraft(raw: unknown): import("zod").infer<typeof import("../../collaboration/public/social-home").postDraftSchema>;
    mentions(raw: unknown): import("zod").infer<typeof import("../../collaboration/public/social-text").mentionsSchema>;
    pmsHotelSources: typeof import("../public/pms/home").pmsHotelSources;
    pmsWorkspace: typeof import("../public/pms").pmsWorkspace;
    pmsHome: typeof import("../public/pms/home").pmsHome;
}) {
    async function assertPostScope(actor: WorkContext, id: string) {
        const write = await ports.repository(actor), post = await write.readablePost(actor, id);
        if (actor.branchId && post.branch_id && post.branch_id !== actor.branchId || actor.departmentId && post.department_id && post.department_id !== actor.departmentId)
            throw ports.error(404, "POST_NOT_FOUND", "This conversation is unavailable in this workspace.");
    }
    const homeQuery = z.object({ property: z.string().max(160).optional(), view: z.enum(["mine", "team", "handover", "company"]).optional(),
        label: z.enum(["arrivals", "departures", "housekeeping", "inspection", "transfers", "corrections", "refunds", "variance", "shift", "night"]).optional(),
        taskCursor: z.string().max(4096).optional(), before: z.coerce.number().int().positive().optional(), beforeId: z.string().max(160).optional(), post: z.string().max(160).optional() });
    async function pmsHomeRead(actor: WorkContext, path: string[], search: URLSearchParams) {
        ports.require(actor, "pms.read");
        const write = await ports.repository(actor);
        if (path.length === 2 && path[0] === 'hotel-sources')
            return ports.pmsHotelSources(actor, path[1], z.enum(['occupancy', 'adr', 'revpar', 'room_revenue']).parse(search.get('metric')), z.iso.date().parse(search.get('businessDate')), z.coerce.number().int().min(1).max(100000).default(1).parse(search.get('page') ?? undefined));
        if (path.length === 2 && path[0] === 'hotel-record')
            return ports.pmsWorkspace(actor, z.string().min(1).parse(search.get('property')), false, true, { view: 'detail', id: path[1] });
        if (!path.length) {
            const query = homeQuery.parse(Object.fromEntries(search));
            if (query.post)
                await assertPostScope(actor, query.post);
            let loaded: string[] | undefined;
            if (search.has("loaded")) {
                try {
                    loaded = z.array(z.string().max(160)).max(100).parse(JSON.parse(search.get("loaded")!));
                }
                catch {
                    throw ports.error(422, "INVALID_FEED_IDS", "Refresh the feed to continue.");
                }
            }
            return ports.pmsHome(actor, { propertyId: query.property, view: query.view, label: query.label, taskCursor: query.taskCursor, before: query.before, beforeId: query.beforeId, postId: query.post, loadedIds: loaded });
        }
        if (path.length >= 2 && (path[0] === "posts" || path[0] === "handover"))
            await assertPostScope(actor, path[1]);
        if (path.length === 2 && path[0] === "posts")
            return write.postView(actor, path[1]);
        if (path.length === 3 && path[0] === "posts" && path[2] === "comments")
            return write.postComments(actor, path[1], search.get("cursor") ?? undefined);
        if (path.length === 3 && path[0] === "handover" && path[2] === "acknowledgement")
            return write.handoverAcknowledgement(actor, path[1]);
        if (path.join("/") === "beta/suggestions")
            return write.textSuggestions(actor, z.enum(["@", "#"]).parse(search.get("trigger")), z.string().max(160).parse(search.get("q") ?? ""));
        throw ports.error(404, "NOT_FOUND", "Not found.");
    }
    async function pmsHomeWrite(actor: WorkContext, path: string[], input: unknown, key: string, expectedHandoverVersion?:number) {
        ports.require(actor, "pms.read");
        const write = await ports.repository(actor);
        if (path.length === 1 && path[0] === "publish") {
            const draft = ports.postDraft(input);
            if (draft.purpose === "handover")
                throw ports.error(422, "HANDOVER_SOURCES_REQUIRED", "Use the handover form to choose work and recipients.");
            const saved = await write.savePostDraft(actor, null, draft, key);
            return write.publishPost(actor, saved.id, saved.version, `${key}-publish`);
        }
        if (path.length === 1 && path[0] === "handover")
            return write.publishHandover(actor, { items: [], unavailableSources: [] }, input, key);
        if (path.length === 3 && path[0] === "handover" && path[2] === "acknowledgement") {
            await assertPostScope(actor, path[1]);
            const version=z.number().int().positive().max(Number.MAX_SAFE_INTEGER).optional().parse(expectedHandoverVersion);
            return version===undefined?write.acknowledgeHandover(actor,path[1],key):write.acknowledgeHandoverVersion(actor,path[1],key,version);
        }
        if (path.length === 2 && path[0] === "posts") {
            await assertPostScope(actor, path[1]);
            const action = z.object({ action: z.enum(["comment", "appreciate", "follow", "edit", "retract"]), body: z.string().max(30000).optional(),
                active: z.boolean().optional(), expectedVersion: z.number().int().positive().optional(), mentions: z.unknown().transform(value => ports.mentions(value)).optional(), richContent: z.unknown().optional() }).strict().parse(input);
            return write.postAction(actor, path[1], action, key);
        }
        throw ports.error(404, "NOT_FOUND", "Not found.");
    }
    return { pmsHomeRead, pmsHomeWrite };
}
