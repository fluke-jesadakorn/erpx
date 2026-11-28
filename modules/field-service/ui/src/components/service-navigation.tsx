import { FeatureTabs } from "@erpx/platform-shared-ui/components/features/feature-page";
import type { NavigationContext } from "@erpx/work-projection-ui/lib/workplace-navigation";
export function ServiceNavigation({}: { context: NavigationContext; groupId: string; activeId: string; exclude?: string[] }) { return <FeatureTabs/>; }
