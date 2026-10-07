export type SocialActivityEvent = {
    id: string;
    source_id: string;
    source_type: "reply" | "mention";
    title: string;
    at: number;
    version: number;
    activity_actor: string;
    thread_id: string;
    href: string;
};
