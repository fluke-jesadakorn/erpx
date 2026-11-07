/** Inbox rendering data; it carries no command authority. */
export type InboxCategory = "work" | "calendar" | "mention" | "notice" | "system";
export type InboxItem = {
    id: string;
    eventId: string;
    category: InboxCategory;
    message: string;
    href: string | null;
    createdAt: number;
    readAt: number | null;
    archivedAt: number | null;
    sourceType: string;
    sourceId: string;
    source?: {kind:string;id:string;version?:number|string;href:string;label:string};
};
