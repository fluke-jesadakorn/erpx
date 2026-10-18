"use client";
import WorkspaceLink from "@/components/workplace/workspace-link";
import {useAccountPreferences} from "@/components/identity_access/account-preferences-context";
export function ServicePartsEntryLink({base}:{base:string}){
 const th=useAccountPreferences()?.preference?.language==="th";
 return <p><WorkspaceLink href={`${base}/services/field-service/settings/parts`}>{th?"เตรียมบัญชี อะไหล่ และสต็อกสำหรับงานบริการ":"Prepare Service parts, stock and posting"}</WorkspaceLink></p>;
}
