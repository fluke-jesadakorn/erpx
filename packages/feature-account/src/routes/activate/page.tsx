
import { PageContent } from "@erpx/ui/components/patterns";
import { inspectInvitation } from "@erpx/server-identity_access/public/account-access";
import { ActivationForm } from "@erpx/workplace/routes/activate/activation-form";
export const dynamic="force-dynamic";
export default async function ActivatePage({searchParams}:{searchParams:Promise<{token?:string}>}){const {token}=await searchParams;let invite;try{invite=await inspectInvitation(token??"");}catch{return <PageContent as="main"><h1>Invitation unavailable</h1><p>The link has expired or was already used. Ask your administrator for a new invitation.</p></PageContent>;}return <PageContent as="main"><h1>Activate your ERPX account</h1><p>{invite.name} · {invite.email}</p><ActivationForm token={token!} existingAccount={invite.existingAccount}/></PageContent>;}
