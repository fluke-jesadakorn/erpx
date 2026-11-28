export type NativeManufacturingBomLine = {
  id: string;
  specification_id: string;
  product_id: string;
  quantity_milli: number;
  substitution_group: string | null;
  ordinal: number;
};

export type NativeManufacturingBomOperation = {
  id: string;
  specification_id: string;
  resource_id: string;
  resource_name: string;
  name: string;
  setup_minutes: number;
  run_minutes_per_batch: number;
  ordinal: number;
};

export type NativeManufacturingBomSpecification = {
  id: string;
  company_id: string;
  name: string;
  mode: 'bom' | 'formula';
  output_product_id: string;
  batch_quantity_milli: number;
  expected_yield_bps: number;
  status: 'draft' | 'approved' | 'rejected' | 'retired';
  created_by: string;
  reviewed_by: string | null;
  version: number;
  created_at: number;
  updated_at: number;
  lines: NativeManufacturingBomLine[];
  operations: NativeManufacturingBomOperation[];
};

export type NativeManufacturingBom = {
  specifications: NativeManufacturingBomSpecification[];
};
