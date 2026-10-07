
import { PageContent } from "@/components/ui/patterns";
import { FeaturePage } from "@/components/features/feature-page";
import { PushSettings } from "@/components/settings/push-settings";
import { PageHeader } from "@/components/ui/primitives";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/modules/identity_access/public/authorization";

export default async function NotificationSettings({params}:PageProps<"/holdings/[holdingSlug]/settings/notifications">){
  const actor=await requireWorkContext((await params).holdingSlug);
  return <PageContent as={FeaturePage} data-source="live">
    <PageHeader appearance="standard" title="Notification settings" description="Choose when this browser should show work alerts."/>
    <PushSettings appearance="standard" base={workspaceBase(publicScope(actor))}/>
  </PageContent>;
}
