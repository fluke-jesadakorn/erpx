import { FeatureTabs } from "@/components/features/feature-page";
import type { NavigationContext } from "@/lib/workplace-navigation";
export function ServiceNavigation(_props: { context: NavigationContext; groupId: string; activeId: string; exclude?: string[] }) { return <FeatureTabs/>; }
