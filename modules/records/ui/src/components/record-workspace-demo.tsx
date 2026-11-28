"use client";
import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { useSearchParams } from "@erpx/platform-shared-ui/lib/destination-search";
import {RecordWorkspaceProvider} from "@erpx/records-ui/components/record-workspace";
import {RecordDrawer} from "@erpx/records-ui/components/record-drawer";
import {RecordScopeProvider} from "@erpx/records-ui/public/client";
import {RecordLink} from "@erpx/records-ui/components/record-link";
import {SectionWorkspace} from "@erpx/platform-workplace/components/section-workspace";
import {FormDisclosure} from "@erpx/platform-ui/components/form-disclosure";
import {Field,TextInput} from "@erpx/platform-ui/components/foundation";
/** Development gallery fixture exercises the real controller without business writes. */
export function RecordWorkspaceDemo(){
 const query=useSearchParams();
 return <RecordWorkspaceProvider><RecordScopeProvider base={canonicalUiHref("/holdings/acme")}><SectionWorkspace title="Record workspace contract" selected="records" items={[{id:'records',title:'Records',href:'/dev/ui'}]}><RecordLink href={canonicalUiHref("/holdings/acme/assets/workspace-fixture")}>Workspace fixture</RecordLink></SectionWorkspace>{query.get('detailId')==='workspace-fixture'&&<RecordDrawer title="Workspace fixture" fullHref="/dev/ui"><h1>Workspace fixture</h1><form onSubmit={event=>event.preventDefault()}><Field appearance="standard" label="Placement"><TextInput appearance="standard"/></Field></form><FormDisclosure appearance="standard"><summary>Edit fixture</summary><form onSubmit={event=>event.preventDefault()}><Field appearance="standard" label="Fixture name"><TextInput appearance="standard"/></Field></form></FormDisclosure></RecordDrawer>}</RecordScopeProvider></RecordWorkspaceProvider>;
}
