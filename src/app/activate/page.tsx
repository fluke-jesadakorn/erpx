import { inspectInvitation } from "@/server/account-access";
import { ActivationForm } from "./activation-form";
export const dynamic="force-dynamic";
export default async function ActivatePage({searchParams}:{searchParams:Promise<{token?:string}>}){const {token}=await searchParams;let invite;try{invite=await inspectInvitation(token??"");}catch{return <main className="content"><h1>Invitation unavailable</h1><p>The link has expired or was already used. Ask your administrator for a new invitation.</p></main>;}return <main className="content"><h1>Activate your ERPX account</h1><p>{invite.name} · {invite.email}</p><ActivationForm token={token!}/></main>;}
