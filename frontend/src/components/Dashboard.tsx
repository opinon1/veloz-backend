import { useCallback, useEffect, useState } from "react";
import { listCharts, ApiError } from "../api";
import type { ChartDef } from "../types";
import ChartCard from "./ChartCard";
import NewChartForm from "./NewChartForm";
import { useToast } from "./Toast";

export default function Dashboard() {
  const toast = useToast();
  const [charts, setCharts] = useState<ChartDef[]>([]);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setCharts(await listCharts());
    } catch (e) {
      toast(e instanceof ApiError ? e.message : "failed to load charts", "err");
    } finally {
      setLoading(false);
    }
  }, [toast]);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <section>
      <div className="toolbar">
        <button className="btn ghost sm" onClick={load}>
          ↻ Refresh all
        </button>
      </div>
      <NewChartForm onCreated={load} />
      {loading && <div className="hint">Loading…</div>}
      <div className="grid">
        {charts.map((def) => (
          <ChartCard key={def.id} def={def} onDeleted={load} />
        ))}
      </div>
    </section>
  );
}
