import { z } from "zod";
import { customerPortalInvitationEnrollment, releaseCustomerPortalEnrollmentAttempt, acceptCustomerPortalInvitation } from "@/server/modules/customer_authority/public/portal-invitation-enrollment";
import { currentSession } from "@/server/modules/identity_access/public/authorization";
import { withCurrentPrivateSessionRead } from "@/server/modules/identity_access/public/private-session-read";
import { createCustomerPortalSessionApplication } from "@/server/modules/customer_360/public/verified-portal-account";
import { createAuth, invitationVerificationOutcome } from "@/server/modules/identity_access/public/auth";
import { configuredProductOrigin, productHostForHost } from "@/lib/runtime/product-hosts";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";

export const dynamic = "force-dynamic";
const responseHeaders = { "Cache-Control": "private, no-store", "Referrer-Policy": "no-referrer", "X-Content-Type-Options": "nosniff" };
const bodySchema = z.discriminatedUnion("action",[
  z.object({action:z.literal("signup"),token:z.string().min(1).max(2048),name:z.string().trim().min(1).max(100),password:z.string().min(12).max(200)}),
  z.object({action:z.literal("accept"),token:z.string().min(1).max(2048)}),
]);
async function boundedPortalBody(request:Request,limit:number){
  const reader=request.body?.getReader();if(!reader)return new Uint8Array();const chunks:Uint8Array[]=[];let total=0;
  while(true){const {done,value}=await reader.read();if(done)break;total+=value.byteLength;if(total>limit){await reader.cancel();throw new ApiError(413,"BODY_TOO_LARGE","The request body is too large.");}chunks.push(value);}
  const body=new Uint8Array(total);let offset=0;for(const chunk of chunks){body.set(chunk,offset);offset+=chunk.byteLength;}return body;
}

export async function POST(request: Request) {
  try {
    const env=await runtimeEnv(),origin = requestPublicOrigin(request), accountOrigin = configuredProductOrigin("account")??env.BETTER_AUTH_URL, hostOwner=productHostForHost(request.headers.get("host"));
    if (origin!==accountOrigin || new URL(request.url).origin!==accountOrigin || request.headers.get("host")!==new URL(accountOrigin).host || hostOwner!==null&&hostOwner!=="account")
      throw new ApiError(404,"NOT_FOUND","Not found.");
    if (request.headers.get("origin")!==origin) throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the Account application origin.");
    let raw: unknown;
    try { raw = JSON.parse(new TextDecoder().decode(await boundedPortalBody(request,16000))); }
    catch(error) { if(error instanceof SyntaxError)throw new ApiError(422,"INVALID_JSON","Invalid JSON.");throw error; }
    const input=bodySchema.parse(raw);
    if(input.action==="signup") {
      if(!env.RESEND_API_KEY||!env.RESEND_FROM)throw new ApiError(503,"EMAIL_NOT_CONFIGURED","Invitation account verification is not configured.");
      const invite=await customerPortalInvitationEnrollment(input.token);
      let reservationReleased=false;
      const releaseReservation=async()=>{if(!reservationReleased){await releaseCustomerPortalEnrollmentAttempt(invite.reservation);reservationReleased=true;}};
      if(invite.attempts>3){await releaseReservation();throw new ApiError(429,"INVITATION_RATE_LIMIT","Too many attempts. Check the invited email or ask the sender to issue a new link.");}
      try{
        const callbackURL=new URL("/customer-portal/enroll?verified=1",origin).href,auth=await createAuth(origin,undefined,invite.email),headers=new Headers(request.headers);
        headers.delete("content-length");headers.set("content-type","application/json");
        const response=await auth.handler(new Request(`${origin}/api/auth/sign-up/email`,{method:"POST",headers,body:JSON.stringify({name:input.name,email:invite.email,password:input.password,callbackURL})}));
        const verification=await invitationVerificationOutcome(auth);
        if(verification==='failed'){
          await releaseReservation();
          return Response.json({error:{code:'VERIFICATION_EMAIL_DELIVERY_FAILED',message:'Your account was created, but its verification email could not be sent. Use Resend verification email for the invited address.'}},{status:503,headers:responseHeaders});
        }
        if(response.ok&&verification!=='sent'){
          await releaseReservation();
          return Response.json({error:{code:'VERIFICATION_EMAIL_DELIVERY_FAILED',message:'Your account could not be confirmed by email. Request a fresh verification email for the invited address and try again.'}},{status:503,headers:responseHeaders});
        }
        if(response.status>=500)await releaseReservation();
        const outputHeaders=new Headers(response.headers);for(const [name,value] of Object.entries(responseHeaders))outputHeaders.set(name,value);
        return new Response(response.body,{status:response.status,statusText:response.statusText,headers:outputHeaders});
      }catch(error){await releaseReservation();throw error;}
    }
    const {session,requestHeaders}=await currentSession(),application=createCustomerPortalSessionApplication({error:(status,code,message)=>new ApiError(status,code,message)}),user=application.verify(session.user),captured=JSON.stringify({session,headers:[...requestHeaders.entries()]});
    const sessionHost=productHostForHost(requestHeaders.get("host"));if(sessionHost!==null&&sessionHost!=="account")throw new ApiError(404,"NOT_FOUND","Not found.");
    const assertCurrent=async()=>{if(JSON.stringify({session,headers:[...requestHeaders.entries()]})!==captured)throw new ApiError(401,"SESSION_REVOKED","Sign in again to accept this invitation.");};
    return await withCurrentPrivateSessionRead({userId:user.userId,email:user.email,sessionId:session.session.id,audience:sessionHost},async assertSession=>{
      const assertRequestCurrent=async()=>{await assertCurrent();await assertSession();};
      const data=await acceptCustomerPortalInvitation(input.token,user,assertRequestCurrent);
      return Response.json({data},{headers:responseHeaders});
    });
  } catch(error) { return errorResponse(error); }
}
