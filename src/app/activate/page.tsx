
import { PageContent } from "@/components/ui/patterns";
import { inspectInvitation } from "@/server/account-access";
import { ActivationForm } from "./activation-form";
export const dynamic="force-dynamic";
export default async function ActivatePage({searchParams}:{searchParams:Promise<{token?:string}>}){const {token}=await searchParams;let invite;try{invite=await inspectInvitation(token??"");}catch{return <PageContent as="main"><h1>Invitation unavailable</h1><p>The link has expired or was already used. Ask your administrator for a new invitation.</p></PageContent>;}return <PageContent as="main"><h1>Activate your ERPX account</h1><p>{invite.name} · {invite.email}</p><ActivationForm token={token!}/></PageContent>;}
