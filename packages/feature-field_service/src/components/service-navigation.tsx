import { FeatureTabs } from "@erpx/feature-shared/components/features/feature-page";
import type { NavigationContext } from "@erpx/feature-work_projection/lib/workplace-navigation";
export function ServiceNavigation({}: { context: NavigationContext; groupId: string; activeId: string; exclude?: string[] }) { return <FeatureTabs/>; }
