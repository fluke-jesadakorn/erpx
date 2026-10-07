import "server-only";
import summary from "@/data/hospitality-evidence.generated.json";
import { productionAccepted as accepted } from "../application/hospitality-evidence";
export function productionAccepted(moduleId: string) { return accepted(summary[moduleId as keyof typeof summary]); }
