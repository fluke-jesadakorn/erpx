import {z} from "zod";
import {BusinessRuleError} from "../../shared";
export const profileInput = z.object({
  bio: z.string().trim().max(500),
  links: z.array(z.object({
    label: z.string().trim().min(1).max(40),
    url: z.url().max(500).refine(value => new URL(value).protocol === "https:", "Use an HTTPS link."),
  }).strict()).max(3),
  version: z.number().int().min(0),
}).strict();

export type ProfileRow = {
  user_id: string; bio: string; links_json: string;
  avatar_key: string | null; cover_key: string | null;
  avatar_media_type: string | null; cover_media_type: string | null;
  version: number;
};


export type MemberProfileInput=z.infer<typeof profileInput>;
export function assertActiveProfile(active:boolean){if(!active)throw new BusinessRuleError(404,'PROFILE_NOT_FOUND','This profile is unavailable.');}
export function profilePresentation<Post extends {id:string;created_at:number}>(actor:{holdingSlug:string;userId:string},userId:string,account:{name:string}|null,profile:ProfileRow|null,work:{company_name:string;job_title:string|null}[],postIds:{id:string}[],posts:Post[]){
 if(!account)throw new BusinessRuleError(404,'PROFILE_NOT_FOUND','This profile is unavailable.');return {userId,name:account.name,bio:profile?.bio??'',links:profile?JSON.parse(profile.links_json) as {label:string;url:string}[]:[],avatarUrl:profile?.avatar_key?`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/profiles/${encodeURIComponent(userId)}/media/avatar?v=${profile.version}`:null,coverUrl:profile?.cover_key?`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/profiles/${encodeURIComponent(userId)}/media/cover?v=${profile.version}`:null,work:work.map(row=>({company:row.company_name,title:row.job_title})),version:profile?.version??0,own:userId===actor.userId,posts,nextBefore:postIds.length>8?posts.at(-1)?.created_at??null:null,nextBeforeId:postIds.length>8?posts.at(-1)?.id??null:null};
}
export function assertProfileMedia(row:{key:string|null;media_type:string|null}|null){if(!row?.key||!row.media_type)throw new BusinessRuleError(404,'PROFILE_MEDIA_NOT_FOUND','Profile image unavailable.');return row;}
