"use client";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { useSearchParams } from "@/lib/shared/destination-search";
import {RecordWorkspaceProvider} from "./record-workspace";
import {RecordDrawer} from "./record-drawer";
import {RecordScopeProvider} from "./record-context";
import {RecordLink} from "./record-link";
import {SectionWorkspace} from "@/components/workplace/section-workspace";
import {FormDisclosure} from "@/components/ui/form-disclosure";
import {Field,TextInput} from "@/components/ui/foundation";
/** Development gallery fixture exercises the real controller without business writes. */
export function RecordWorkspaceDemo(){
 const query=useSearchParams();
 return <RecordWorkspaceProvider><RecordScopeProvider base={canonicalUiHref("/holdings/acme")}><SectionWorkspace title="Record workspace contract" selected="records" items={[{id:'records',title:'Records',href:'/dev/ui'}]}><RecordLink href={canonicalUiHref("/holdings/acme/assets/workspace-fixture")}>Workspace fixture</RecordLink></SectionWorkspace>{query.get('detailId')==='workspace-fixture'&&<RecordDrawer title="Workspace fixture" fullHref="/dev/ui"><h1>Workspace fixture</h1><form onSubmit={event=>event.preventDefault()}><Field appearance="standard" label="Placement"><TextInput appearance="standard"/></Field></form><FormDisclosure appearance="standard"><summary>Edit fixture</summary><form onSubmit={event=>event.preventDefault()}><Field appearance="standard" label="Fixture name"><TextInput appearance="standard"/></Field></form></FormDisclosure></RecordDrawer>}</RecordScopeProvider></RecordWorkspaceProvider>;
}
