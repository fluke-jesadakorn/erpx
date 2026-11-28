export type NativePlanningShortage = {
  productId: string;
  requiredMilli: number;
  availableMilli: number;
  incomingMilli: number;
  shortageMilli: number;
};

export type NativePlanningContext = {
  demandSource?: {
    id: string;
    version: number;
    quantityMilli: number;
  };
  incomingSupply: {
    id: string;
    version: number;
    productId: string;
    quantityMilli: number;
    dueDate: string;
  }[];
  stock: { productId: string; version: number }[];
  allocations: {
    resourceId: string;
    workDate: string;
    minutes: number;
    resourceVersion: number;
    calendarVersion: number;
    availableMinutes: number;
  }[];
  operations: {
    id: string;
    name: string;
    resourceId: string;
    requiredMinutes: number;
  }[];
  startDate?: string;
  capacityBasis?: string;
  comparisonScenarioId?: string;
  comparison?: {
    scenarioId: string;
    sourceVersion: number;
    shortageDeltaMilli: number;
    requiredMinutesDelta: number;
    feasibleDate: string;
    currentFeasibleDate: string;
  };
};

export type NativePlanningScenario = {
  id: string;
  company_id: string;
  branch_id: string;
  warehouse_id: string;
  specification_id: string;
  specification_version: number;
  demand_quantity_milli: number;
  due_date: string;
  capacity_minutes: number;
  status: 'calculated' | 'released' | 'cancelled';
  created_by: string;
  version: number;
  created_at: number;
  updated_at: number;
  shortages: NativePlanningShortage[];
  context: NativePlanningContext;
};

export type NativePlanningRecommendation = {
  id: string;
  company_id: string;
  branch_id: string;
  scenario_id: string;
  scenario_version: number;
  recommended_quantity_milli: number;
  required_minutes: number;
  feasible_date: string;
  status: 'proposed' | 'approved' | 'rejected';
  created_by: string;
  reviewed_by: string | null;
  version: number;
  created_at: number;
  updated_at: number;
};

export type NativePlanningResource = {
  id: string;
  company_id: string;
  branch_id: string;
  code: string;
  name: string;
  daily_minutes: number;
  work_weekdays: number[];
  status: 'active' | 'archived';
  version: number;
  created_by: string;
  created_at: number;
  updated_at: number;
};

export type NativePlanningCalendarDay = {
  resource_id: string;
  work_date: string;
  available_minutes: number;
  reason: string;
  version: number;
  updated_by: string;
  updated_at: number;
};

export type NativePlanningAllocation = {
  id: string;
  scenario_id: string;
  resource_id: string;
  work_date: string;
  minutes: number;
  created_by: string;
  created_at: number;
};

export type NativePlanningWorkspace = {
  scenarios: NativePlanningScenario[];
  recommendations: NativePlanningRecommendation[];
  resources: NativePlanningResource[];
  calendar: NativePlanningCalendarDay[];
  allocations: NativePlanningAllocation[];
};
