export type ProjectCapability="projects.wbs"|"projects.tasks"|"projects.time"|"projects.handover"|"projects.costs";
export type ProjectPurchaseAccess={newWork:boolean;settleExisting:boolean;history:boolean;revision:string};
export type ProjectPurchaseControls=Partial<Record<ProjectCapability,ProjectPurchaseAccess>>;
export function projectPurchased(controls:ProjectPurchaseControls|undefined,capability:ProjectCapability,purpose:"newWork"|"settleExisting"|"history"){
 return controls===undefined||Boolean(controls[capability]?.[purpose])&&(capability==="projects.wbs"||Boolean(controls["projects.wbs"]?.[purpose]));
}
export function projectStructurePurchased(controls:ProjectPurchaseControls|undefined,action:string,sourceKind?:string){
 const capability=action==="link-actual"?(sourceKind==="cost_allocation"?"projects.costs":"projects.time"):action.includes("handoff")||action==="reopen"?"projects.handover":"projects.wbs";
 return projectPurchased(controls,capability,["assign-task","reopen"].includes(action)?"newWork":"settleExisting");
}
