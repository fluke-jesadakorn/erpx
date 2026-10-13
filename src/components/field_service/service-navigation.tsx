import { FeatureTabs } from "@/components/shared/features/feature-page";
import type { NavigationContext } from "@/lib/work_projection/workplace-navigation";
export function ServiceNavigation({}: { context: NavigationContext; groupId: string; activeId: string; exclude?: string[] }) { return <FeatureTabs/>; }
