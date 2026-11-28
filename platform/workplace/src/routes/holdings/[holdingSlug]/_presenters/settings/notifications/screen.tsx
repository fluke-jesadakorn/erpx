
import { PageContent } from "@erpx/platform-ui/components/patterns";
import { FeaturePage } from "@erpx/platform-shared-ui/components/features/feature-page";
import { PushSettings } from "@erpx/platform-administration-ui/components/settings/push-settings";
import { PageHeader } from "@erpx/platform-ui/components/primitives";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope,requireWorkContext } from "@erpx/identity-access-server/public/authorization";

export default async function NotificationSettings({params}:{ params: Promise<{ holdingSlug: string;  }>; searchParams: Promise<Record<string,string|string[]|undefined>> }){
  const actor=await requireWorkContext((await params).holdingSlug);
  return <PageContent as={FeaturePage} data-source="live">
    <PageHeader appearance="standard" title="Notification settings" description="Choose when this browser should show work alerts."/>
    <PushSettings appearance="standard" base={workspaceBase(publicScope(actor))}/>
  </PageContent>;
}
