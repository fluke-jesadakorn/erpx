import "server-only";
import summary from "@erpx/catalogue/hospitality-evidence.generated.json";
import { productionAccepted as accepted } from "@erpx/server-pms/application/hospitality-evidence";
export function productionAccepted(moduleId: string) { return accepted(summary[moduleId as keyof typeof summary]); }
