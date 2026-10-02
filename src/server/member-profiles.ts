import "server-only";

import { z } from "zod";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { runtimeEnv, holdingResources } from "./runtime";
import type { WorkContext } from "./authorization";
import { socialPostScope } from "./social-source";
import { postViews } from "./social-home";

export const profileInput = z.object({
  bio: z.string().trim().max(500),
  links: z.array(z.object({
    label: z.string().trim().min(1).max(40),
    url: z.url().max(500).refine(value => new URL(value).protocol === "https:", "Use an HTTPS link."),
  }).strict()).max(3),
  version: z.number().int().min(0),
}).strict();

type ProfileRow = {
  user_id: string; bio: string; links_json: string;
  avatar_key: string | null; cover_key: string | null;
  avatar_media_type: string | null; cover_media_type: string | null;
  version: number;
};

export async function activeHoldingMember(actor: WorkContext, userId: string) {
  const { write } = await holdingResources(actor.holdingId);
  const now = Date.now();
  const member = await write.prepare(`SELECT 1 FROM assignments a
    JOIN organization_nodes c ON c.id=a.company_id AND c.kind='company' AND c.status='active'
    WHERE a.user_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>?
      AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
    LIMIT 1`).bind(userId,now,now,now,now).first();
  if (!member) throw new ApiError(404, "PROFILE_NOT_FOUND", "This profile is unavailable.");
  return write;
}

export async function memberProfileView(actor: WorkContext, userId: string, before?: number, beforeId?: string) {
  const write = await activeHoldingMember(actor, userId);
  const env = await runtimeEnv();
  const postScope = socialPostScope(actor);
  const [account, profile, work, postIds] = await Promise.all([
    env.CONTROL_DB.prepare('SELECT id,name FROM "user" WHERE id=?').bind(userId).first<{id:string;name:string}>(),
    write.prepare("SELECT user_id,bio,links_json,avatar_key,cover_key,avatar_media_type,cover_media_type,version FROM member_profiles WHERE user_id=?").bind(userId).first<ProfileRow>(),
    write.prepare(`SELECT c.display_name AS company_name,COALESCE(op.title,e.job_title) AS job_title
      FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
      JOIN organization_nodes c ON c.id=e.company_id AND c.kind='company' AND c.status='active'
      LEFT JOIN organization_positions op ON op.id=e.position_id AND op.active=1
      WHERE (p.user_id=? OR EXISTS(SELECT 1 FROM iam_members m WHERE m.person_id=p.id AND m.user_id=? AND m.company_id=p.company_id))
        AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'
      ORDER BY e.start_date DESC LIMIT 3`).bind(userId,userId).all<{company_name:string;job_title:string|null}>(),
    write.prepare(`SELECT p.id FROM social_posts p WHERE p.author_id=? AND ${postScope.sql}
      AND (?::bigint IS NULL OR p.created_at<? OR (p.created_at=? AND (?::text IS NULL OR p.id<?)))
      ORDER BY p.created_at DESC,p.id DESC LIMIT 9`)
      .bind(userId,...postScope.bindings,before??null,before??null,before??null,beforeId??null,beforeId??null).all<{id:string}>(),
  ]);
  if (!account) throw new ApiError(404, "PROFILE_NOT_FOUND", "This profile is unavailable.");
  const ids = postIds.results.slice(0,8).map(row => row.id);
  const posts = await postViews(write,actor,ids,20);
  return {
    userId, name: account.name, bio: profile?.bio ?? "",
    links: profile ? JSON.parse(profile.links_json) as {label:string;url:string}[] : [],
    avatarUrl: profile?.avatar_key ? `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/profiles/${encodeURIComponent(userId)}/media/avatar?v=${profile.version}` : null,
    coverUrl: profile?.cover_key ? `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/profiles/${encodeURIComponent(userId)}/media/cover?v=${profile.version}` : null,
    work: work.results.map(row => ({ company: row.company_name, title: row.job_title })),
    version: profile?.version ?? 0, own: userId === actor.userId,
    posts, nextBefore: postIds.results.length > 8 ? posts.at(-1)?.created_at ?? null : null,
    nextBeforeId: postIds.results.length > 8 ? posts.at(-1)?.id ?? null : null,
  };
}

export async function saveMemberProfile(actor: WorkContext, input: z.infer<typeof profileInput>, commandId: string) {
  const write = await activeHoldingMember(actor, actor.userId);
  const nextVersion = input.version + 1;
  const result = { version: nextVersion, bio: input.bio, links: input.links };
  const prior = await replayCommand<typeof result>(write,actor,commandId,"member.profile.update",actor.userId,input);
  if (prior) return prior;
  const now = Date.now();
  return executeCommand(write,actor,commandId,"member.profile.update",input,actor.userId,nextVersion,result,[
    assertion(write,commandId,"profile-version",input.version===0
      ? "NOT EXISTS(SELECT 1 FROM member_profiles WHERE user_id=?)"
      : "EXISTS(SELECT 1 FROM member_profiles WHERE user_id=? AND version=?)",
      input.version===0?[actor.userId]:[actor.userId,input.version]),
    input.version===0
      ? write.prepare("INSERT INTO member_profiles(user_id,bio,links_json,version,updated_at) VALUES(?,?,?::jsonb,?,?)")
        .bind(actor.userId,input.bio,JSON.stringify(input.links),nextVersion,now)
      : write.prepare("UPDATE member_profiles SET bio=?,links_json=?::jsonb,version=?,updated_at=? WHERE user_id=? AND version=?")
        .bind(input.bio,JSON.stringify(input.links),nextVersion,now,actor.userId,input.version),
  ]);
}

export async function profileMediaRow(actor: WorkContext, userId: string, kind: "avatar"|"cover") {
  const write = await activeHoldingMember(actor,userId);
  const row = await write.prepare(`SELECT ${kind}_key AS key,${kind}_media_type AS media_type FROM member_profiles WHERE user_id=?`)
    .bind(userId).first<{key:string|null;media_type:string|null}>();
  if (!row?.key || !row.media_type) throw new ApiError(404,"PROFILE_MEDIA_NOT_FOUND","Profile image unavailable.");
  return row;
}
