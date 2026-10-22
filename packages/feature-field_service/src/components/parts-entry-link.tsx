"use client";
import WorkspaceLink from "@erpx/workplace/components/workspace-link";
import {useAccountPreferences} from "@erpx/feature-identity_access/components/account-preferences-context";
export function ServicePartsEntryLink({base}:{base:string}){
 const th=useAccountPreferences()?.preference?.language==="th";
 return <p><WorkspaceLink href={`${base}/services/field-service/settings/parts`}>{th?"เตรียมบัญชี อะไหล่ และสต็อกสำหรับงานบริการ":"Prepare Service parts, stock and posting"}</WorkspaceLink></p>;
}
