
import { PageContent } from "@erpx/ui/components/patterns";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { PushSettings } from "@erpx/feature-platform_administration/components/settings/push-settings";
import { PageHeader } from "@erpx/ui/components/primitives";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope,requireWorkContext } from "@erpx/server-identity_access/public/authorization";

export default async function NotificationSettings({params}:{ params: Promise<{ holdingSlug: string;  }>; searchParams: Promise<Record<string,string|string[]|undefined>> }){
  const actor=await requireWorkContext((await params).holdingSlug);
  return <PageContent as={FeaturePage} data-source="live">
    <PageHeader appearance="standard" title="Notification settings" description="Choose when this browser should show work alerts."/>
    <PushSettings appearance="standard" base={workspaceBase(publicScope(actor))}/>
  </PageContent>;
}
