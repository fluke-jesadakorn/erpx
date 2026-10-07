import { z } from "zod";
import type { WorkContext } from "../../identity_access/public/authorization";
type DropDatabase<Fn> = Fn extends (database: infer _Database, ...args: infer Args) => infer Result ? (...args: Args) => Result : never;
type DropStorage<Fn> = Fn extends (database: infer _Database, storage: infer _Storage, ...args: infer Args) => infer Result ? (...args: Args) => Result : never;
export type FrontPmsFileContent = Awaited<ReturnType<typeof import("../../files/public/attachments/service").attachmentContent>>;
export type FrontPmsAttachmentRepository = {
    setTarget(id: string, companyId: string): Promise<{
        target_type: string;
    } | null>;
    linkSet(id: string, companyId: string): Promise<{
        set_id: string;
    } | null>;
    upload(id: string, companyId: string, userId: string): Promise<{
        attachment_set_id: string;
        declared_bytes: number;
        media_type: string;
    } | null>;
    attachmentContent: DropStorage<typeof import("../../files/public/attachments/service").attachmentContent>;
    attachmentLinkStatus: DropDatabase<typeof import("../../files/public/attachments/service").attachmentLinkStatus>;
    bindRecordAttachmentSet: DropDatabase<typeof import("../../files/public/attachments/service").bindRecordAttachmentSet>;
    completeAttachmentUpload: DropStorage<typeof import("../../files/public/attachments/service").completeAttachmentUpload>;
    createAttachmentSet: DropDatabase<typeof import("../../files/public/attachments/service").createAttachmentSet>;
    listAttachmentSets: DropDatabase<typeof import("../../files/public/attachments/service").listAttachmentSets>;
    reserveAttachmentFile: DropDatabase<typeof import("../../files/public/attachments/service").reserveAttachmentFile>;
    withdrawAttachmentLink: DropDatabase<typeof import("../../files/public/attachments/service").withdrawAttachmentLink>;
};
export function createFrontPmsAttachments<Reply>(ports: {
    repository(actor: WorkContext): Promise<FrontPmsAttachmentRepository>;
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
    origin(request: Request): string;
    readBody: typeof import("../../files/public/attachments/upload").readBoundedUploadBody;
    reply(data: unknown, status: number): Reply;
    content(result: FrontPmsFileContent): Reply;
}) {
    const target = z.enum(['pms_task', 'pms_guest', 'pms_reservation', 'pms_stay', 'pms_lost_property']);
    const create = z.object({ targetType: target, targetId: z.string().min(1).max(180), componentKind: z.enum(['record', 'action']), componentRef: z.string().max(180).optional(), purposeKey: z.string().min(1).max(80), expectedTargetVersion: z.number().int().positive() });
    const frontPmsAttachmentPaths = new Set(['attachment-sets', 'attachment-links', 'uploads']);
    /** Narrow Front adapter. All target, revision, file and branch guards remain with Files. */
    async function frontPmsAttachmentRequest(request: Request, actor: WorkContext, path: string[]) {
        ports.require(actor, 'pms.read');
        const write = await ports.repository(actor), [kind, id, action] = path;
        const query = new URL(request.url).searchParams;
        const reply = (data: unknown, status = 200) => ports.reply(data, status);
        const key = () => { const value = request.headers.get('Idempotency-Key'); if (!value || !/^[a-zA-Z0-9:_.-]{8,180}$/.test(value))
            throw ports.error(422, 'COMMAND_KEY_REQUIRED', 'A valid Idempotency-Key is required.'); return value; };
        if (request.method !== 'GET' && request.headers.get('origin') !== ports.origin(request))
            throw ports.error(403, 'ORIGIN_FORBIDDEN', 'Use the application origin.');
        async function setAllowed(setId: string) {
            const row = await write.setTarget(setId, actor.companyId);
            if (!row || !target.safeParse(row.target_type).success)
                throw ports.error(404, 'NOT_FOUND', 'Hotel file set not found.');
        }
        if (kind === 'attachment-sets' && path.length === 1) {
            if (request.method === 'GET') {
                const value = create.pick({ targetType: true, targetId: true }).extend({ purposeKey: z.string().optional(), componentKind: z.enum(['record', 'action']).optional(), expectedTargetVersion: z.coerce.number().int().positive().optional() }).parse(Object.fromEntries(query));
                return reply(await write.listAttachmentSets(actor, value));
            }
            if (request.method === 'POST')
                return reply(await write.createAttachmentSet(actor, create.parse(await request.json()), key()), 201);
        }
        if (kind === 'attachment-sets' && id && path.length === 3 && request.method === 'POST') {
            await setAllowed(id);
            if (action === 'files') {
                const value = z.object({ filename: z.string().min(1).max(255), mediaType: z.string().min(1).max(100), bytes: z.number().int().positive().max(10 * 1024 * 1024), caption: z.string().max(1000).optional(), supersedesLinkId: z.string().min(1).max(180).optional() }).parse(await request.json());
                const result = await write.reserveAttachmentFile(actor, id, value, key());
                const url = new URL(request.url);
                url.search = '';
                url.pathname = url.pathname.replace(/\/attachment-sets\/[^/]+\/files$/, `/uploads/${encodeURIComponent(result.uploadIntentId)}/content`);
                return reply({ ...result, uploadUrl: url.pathname }, 201);
            }
            if (action === 'bind') {
                const value = z.object({ expectedTargetVersion: z.number().int().positive() }).parse(await request.json());
                return reply(await write.bindRecordAttachmentSet(actor, id, value.expectedTargetVersion, key()));
            }
        }
        if (kind === 'attachment-links' && id && path.length === 3) {
            const row = await write.linkSet(id, actor.companyId);
            if (!row)
                throw ports.error(404, 'NOT_FOUND', 'Hotel file link not found.');
            await setAllowed(row.set_id);
            if (action === 'status' && request.method === 'GET')
                return reply(await write.attachmentLinkStatus(actor, id));
            if (action === 'withdraw' && request.method === 'POST')
                return reply(await write.withdrawAttachmentLink(actor, id, z.object({ reason: z.string().min(3).max(1000) }).parse(await request.json()).reason, key()));
            if (action === 'content' && request.method === 'GET') {
                const variant = z.enum(['original', 'preview']).parse(query.get('variant') ?? 'original');
                const result = await write.attachmentContent(actor, id, variant);
                return ports.content(result);
            }
        }
        if (kind === 'uploads' && id && action === 'content' && path.length === 3 && request.method === 'PUT') {
            const row = await write.upload(id, actor.companyId, actor.userId);
            if (!row?.attachment_set_id)
                throw ports.error(404, 'UPLOAD_NOT_FOUND', 'Hotel upload not found.');
            await setAllowed(row.attachment_set_id);
            if (request.headers.get('content-type')?.split(';')[0].trim().toLowerCase() !== row.media_type)
                throw ports.error(422, 'UPLOAD_TYPE', 'Content-Type must match the reserved file type.');
            return reply(await write.completeAttachmentUpload(actor, id, await ports.readBody(request, Number(row.declared_bytes))));
        }
        throw ports.error(404, 'NOT_FOUND', 'Hotel file action not found.');
    }
    return { frontPmsAttachmentRequest, frontPmsAttachmentPaths };
}
