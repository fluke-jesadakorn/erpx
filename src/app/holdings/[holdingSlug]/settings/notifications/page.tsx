import { FeaturePage } from "@/components/features/feature-page";
import { PushSettings } from "@/components/settings/push-settings";
import { PageHeader } from "@/components/ui/primitives";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";

export default async function NotificationSettings({params}:PageProps<"/holdings/[holdingSlug]/settings/notifications">){
  const actor=await requireWorkContext((await params).holdingSlug);
  return <FeaturePage className="content" data-source="live">
    <PageHeader title="Notification settings" description="Choose when this browser should show work alerts."/>
    <PushSettings base={workspaceBase(publicScope(actor))}/>
  </FeaturePage>;
}
