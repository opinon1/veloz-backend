export type ChartType = "table" | "line" | "bar" | "pie" | "stat";

export interface ChartConfig {
  x?: string;
  y?: string;
  series?: string;
}

export interface ChartDef {
  id: string;
  title: string;
  description: string;
  sql: string;
  chart_type: ChartType;
  config: ChartConfig;
  is_builtin: boolean;
  sort_order: number;
}

export interface QueryResult {
  columns: string[];
  rows: Record<string, unknown>[];
  row_count: number;
}

export interface NewChart {
  title: string;
  description?: string;
  sql: string;
  chart_type: ChartType;
  config: ChartConfig;
  sort_order?: number;
}
