import { useCallback, useEffect, useState } from "react";
import { chartData, deleteChart, ApiError } from "../api";
import type { ChartDef, QueryResult } from "../types";
import ChartView from "./ChartView";
import { useToast } from "./Toast";

export default function ChartCard({ def, onDeleted }: { def: ChartDef; onDeleted: () => void }) {
  const toast = useToast();
  const [res, setRes] = useState<QueryResult | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    setErr(null);
    try {
      setRes(await chartData(def.id));
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : "failed to load");
    } finally {
      setLoading(false);
    }
  }, [def.id]);

  useEffect(() => {
    load();
  }, [load]);

  const remove = async () => {
    try {
      await deleteChart(def.id);
      onDeleted();
    } catch (e) {
      toast(e instanceof ApiError ? e.message : "delete failed", "err");
    }
  };

  return (
    <div className="card">
      <div className="card-actions">
        {!def.is_builtin && (
          <button className="btn ghost sm" title="Delete chart" onClick={remove}>
            ✕
          </button>
        )}
        <button className="btn ghost sm" title="Refresh" onClick={load}>
          ↻
        </button>
      </div>
      <h3>{def.title}</h3>
      {def.description && <div className="desc">{def.description}</div>}
      <div className="card-body">
        {loading && <div className="hint">…</div>}
        {err && <div className="err-text">{err}</div>}
        {!loading && !err && res && (
          <ChartView type={def.chart_type} config={def.config} result={res} />
        )}
      </div>
    </div>
  );
}
