import type {AiProviderSnapshot} from '@erpx/server-agent/public/ai-providers';
/** Private owner phase facts. Provider calls deliberately occur between phases. */
export type SocialAiPhase={phase:'reserve'|'finish'|'failure'|'images';reservationId:string;postId:string;sourceHash?:string;provider?:AiProviderSnapshot;images?:{ids:string[];commentId:string|null};reply?:{id:string;leaseToken:string;actorJson:string;ownerType:'post'|'comment';ownerId:string}};
