import type { WorkContext } from '../../identity_access/public/authorization';
import { ASSISTANT_ID, hashtags, validMentionSpan, type Mention, type Suggestion } from '../domain/social-text';
import { mentionsSchema } from '../domain/social-text';
import { BusinessRuleError, type Clock, type IdentifierSource } from '../../shared';
import type { searchReferences } from '../../records/public/records';
export interface SocialTextRepository<Effect> {
    activeMentionAccounts(companyId: string, now: number, now2: number): Promise<{
        results: {
            user_id: string;
        }[];
    }>;
    tagSuggestions(value: string, companyId: string, userId: string, now: number, now2: number): Promise<{
        results: {
            tag: string;
        }[];
    }>;
    assistantName(companyId: string): Promise<{
        home_ai_name: string;
    } | null>;
    currentAssistantName(companyId: string): Promise<{
        home_ai_name: string;
    } | null>;
    previousAssistantName(companyId: string, label: string): Promise<Record<string, unknown> | null>;
    prepareMention(ownerType: "post" | "comment", ownerId: string, kind: "record" | "person" | "agent", targetId: string, label: string, userId: string, now: number): Effect;
    prepareMentionNotice(value: string, targetId: string, now: number, postId: string, targetId2: string, now2: number, now3: number): Effect;
    prepareAssistantReply(value: string, postId: string, ownerType: "post" | "comment", ownerId: string, companyId: string, userId: string, value2: string, name: string, now: number, now2: number): Effect;
    prepareTopic(tag: string, userId: string, now: number): Effect;
    prepareTopicBinding(ownerId: string, tag: string, ownerType: string): Effect;
}
export interface SocialTextPorts<Effect> {
    repository: SocialTextRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    mentionPrincipals(ids: string[]): Promise<{
        user_id: string;
        display_name: string;
    }[]>;
    searchReferences(actor: WorkContext, query: string): ReturnType<typeof searchReferences>;
    getRecord(actor: WorkContext, module: string, id: string): Promise<{
        title: string;
    }>;
    captureJobAuthority(actor: WorkContext): Promise<WorkContext>;
}
export function createSocialTextApplication<Effect>(ports: SocialTextPorts<Effect>) {
    const r = ports.repository;
    async function mentionPeople(actor: Pick<WorkContext, "companyId">) {
        const now = ports.clock.now();
        const ids = (await r.activeMentionAccounts(actor.companyId, now, now)).results;
        if (!ids.length)
            return [];
        return ports.mentionPrincipals(ids.map(person => person.user_id));
    }
    async function textSuggestions(actor: WorkContext, trigger: string, q: string): Promise<Suggestion[]> {
        if (trigger === "#") {
            const now = ports.clock.now();
            return (await r.tagSuggestions(`${q.normalize("NFC").toLowerCase().replace(/[\\%_]/g, "\\$&")}%`, actor.companyId, actor.userId, now, now)).results.map(r => ({ kind: "tag", targetId: r.tag, label: r.tag, detail: "Hashtag" }));
        }
        const needle = q.toLowerCase();
        const people = (await mentionPeople(actor)).filter(p => p.display_name.toLowerCase().includes(needle)).slice(0, 8).map(p => ({ kind: "person" as const, targetId: p.user_id, label: p.display_name, detail: `Person · ${p.user_id}` }));
        const aiName = (await r.assistantName(actor.companyId))?.home_ai_name ?? "AI";
        const agents = ["ai", "erpx assistant", aiName.toLowerCase()].some(name => name.includes(needle)) ? [{ kind: "agent" as const, targetId: ASSISTANT_ID, label: aiName, detail: "AI · replies after publishing" }] : [];
        const records = (await ports.searchReferences(actor, q ? `@${q}` : "@")).filter((r): r is {
            kind: string;
            module_id: string;
            id: string;
            title: string;
        } => r.kind === "record").slice(0, 8).map(r => ({ kind: "record" as const, targetId: `${r.module_id}/${r.id}`, label: String(r.title), detail: `${r.module_id} · ${r.id}` }));
        return [...people, ...agents, ...records];
    }
    async function validateMentions(actor: WorkContext, body: string, input: Mention[] = []) {
        const mentions = mentionsSchema.parse(input).sort((a, b) => a.start - b.start);
        const people = mentions.some(m => m.kind === "person") ? await mentionPeople(actor) : [];
        let end = 0;
        for (const m of mentions) {
            if (m.start < end || m.end > body.length || !validMentionSpan(body, m))
                throw new BusinessRuleError(422, "MENTION_CHANGED", "Choose the mention again after editing its text.");
            end = m.end;
            let label: string | undefined;
            if (m.kind === "person")
                label = people.find(p => p.user_id === m.targetId)?.display_name;
            else if (m.kind === "agent" && m.targetId === ASSISTANT_ID) {
                const current = (await r.currentAssistantName(actor.companyId))?.home_ai_name ?? "AI";
                if ([current, "AI", "ERPX assistant"].includes(m.label))
                    label = m.label;
                else {
                    const old = await r.previousAssistantName(actor.companyId, m.label);
                    if (old)
                        label = m.label;
                }
            }
            else if (m.kind === "record") {
                const [module, ...ids] = m.targetId.split("/");
                label = (await ports.getRecord(actor, module, ids.join("/"))).title;
            }
            if (!label)
                throw new BusinessRuleError(422, "MENTION_UNAVAILABLE", "A mention is no longer available. Remove it or choose another target.");
            if (label !== m.label)
                throw new BusinessRuleError(422, "MENTION_RENAMED", "A mention has been renamed. Choose it again.");
        }
        return mentions;
    }
    async function textStatements(actor: WorkContext, postId: string, ownerType: "post" | "comment", ownerId: string, body: string, mentions: Mention[]) {
        const now = ports.clock.now(), steps = [];
        if (mentions.some(m => m.kind === "agent"))
            actor = await ports.captureJobAuthority(actor);
        for (const m of new Map(mentions.map(m => [`${m.kind}:${m.targetId}`, m])).values()) {
            steps.push(r.prepareMention(ownerType, ownerId, m.kind, m.targetId, m.label, actor.userId, now));
            if (m.kind === "person" && m.targetId !== actor.userId)
                steps.push(r.prepareMentionNotice(ports.identifiers.next(), m.targetId, now, postId, m.targetId, now, now));
            if (m.kind === "agent")
                steps.push(r.prepareAssistantReply(`agent-${ownerId}`, postId, ownerType, ownerId, actor.companyId, actor.userId, JSON.stringify({ userId: actor.userId, assignmentId: actor.assignmentId, role: actor.role, companyId: actor.companyId, branchId: actor.branchId, departmentId: actor.departmentId, iam: actor.iam, authority: actor.authority }), actor.name, now, now));
        }
        for (const tag of new Set(hashtags(body).filter(t => !mentions.some(m => t.start < m.end && t.end > m.start)).map(t => t.tag))) {
            steps.push(r.prepareTopic(tag, actor.userId, now));
            steps.push(r.prepareTopicBinding(ownerId, tag, ownerType));
        }
        return steps;
    }
    return { mentionPeople, textSuggestions, validateMentions, textStatements };
}
