export type Mention = {
    kind: "person" | "agent" | "record";
    targetId: string;
    label: string;
    start: number;
    end: number;
};
export type Suggestion = {
    kind: Mention["kind"] | "tag";
    targetId: string;
    label: string;
    detail: string;
};
export const ASSISTANT_ID = "erpx-assistant";
export const normalizeTag = (tag: string) => tag.normalize("NFC").toLowerCase();
export function hashtags(body: string) {
    return [...body.matchAll(/(?:^|[^\p{L}\p{M}\p{N}_/#])#([\p{L}\p{N}][\p{L}\p{M}\p{N}_-]{0,39})(?![\p{L}\p{M}\p{N}_-])/gu)].map(m => ({ tag: normalizeTag(m[1]), start: m.index! + m[0].length - m[1].length - 1, end: m.index! + m[0].length }));
}
export function activeToken(body: string, caret: number) {
    const match = /(?:^|[\s(])([@#])([^\s@#]*)$/u.exec(body.slice(0, caret));
    return match ? { trigger: match[1], query: match[2], start: caret - match[2].length - 1, end: caret } : null;
}
/** Offsets use JavaScript UTF-16 indices, matching textarea selectionStart. */
export function validMentionSpan(body: string, m: Mention) {
    return body.slice(m.start, m.end) === `@${m.label}` && !/[\p{L}\p{M}\p{N}_]/u.test(body.slice(m.end, m.end + 1));
}
export function editMentions(before: string, after: string, mentions: Mention[]) {
    let start = 0;
    while (start < before.length && start < after.length && before[start] === after[start])
        start++;
    let oldEnd = before.length, newEnd = after.length;
    while (oldEnd > start && newEnd > start && before[oldEnd - 1] === after[newEnd - 1]) {
        oldEnd--;
        newEnd--;
    }
    const delta = newEnd - oldEnd;
    return mentions.flatMap(m => {
        if (m.end <= start)
            return [m];
        if (m.start >= oldEnd)
            return [{ ...m, start: m.start + delta, end: m.end + delta }];
        return [];
    }).filter(m => validMentionSpan(after, m));
}
