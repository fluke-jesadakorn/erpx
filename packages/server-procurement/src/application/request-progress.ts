import { BusinessRuleError } from "@erpx/server-shared/index";
import { projectPurchaseProgress, type PurchaseProgressRequest, type PurchaseProgressFacts } from "@erpx/server-procurement/domain/request-progress";
export interface PurchaseProgressPorts {
  canPurchase: boolean; canFinance: boolean;
  request(id: string): Promise<PurchaseProgressRequest>;
  facts(request: PurchaseProgressRequest): Promise<PurchaseProgressFacts>;
}
export function createPurchaseProgressApplication(ports: PurchaseProgressPorts) {
  return async (id: string) => {
    const request = await ports.request(id);
    if (request.kind !== "purchase") throw new BusinessRuleError(422, "PURCHASE_REQUEST_REQUIRED", "Purchase progress is available only for purchase requests.");
    return projectPurchaseProgress(request, await ports.facts(request), ports.canPurchase, ports.canFinance);
  };
}
