import { useState } from "react";
import { createChart, ApiError } from "../api";
import type { ChartType, NewChart } from "../types";
import { useToast } from "./Toast";

const TYPES: ChartType[] = ["table", "line", "bar", "pie", "stat"];

export default function NewChartForm({
  initialSql = "",
  onCreated,
}: {
  initialSql?: string;
  onCreated: () => void;
}) {
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [type, setType] = useState<ChartType>("table");
  const [sql, setSql] = useState(initialSql);
  const [x, setX] = useState("");
  const [y, setY] = useState("");
  const [series, setSeries] = useState("");
  const [busy, setBusy] = useState(false);

  const showAxes = type !== "table" && type !== "stat";

  const submit = async () => {
    setBusy(true);
    const config: NewChart["config"] = {};
    if (x) config.x = x;
    if (y) config.y = y;
    if (series) config.series = series;
    try {
      await createChart({ title, sql, chart_type: type, config });
      toast("Chart saved", "ok");
      setTitle("");
      setSql("");
      setX("");
      setY("");
      setSeries("");
      setOpen(false);
      onCreated();
    } catch (e) {
      toast(e instanceof ApiError ? e.message : "save failed", "err");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="panel">
      <button className="summary" onClick={() => setOpen((o) => !o)}>
        {open ? "−" : "+"} New chart
      </button>
      {open && (
        <div style={{ marginTop: 14 }}>
          <div className="row">
            <div>
              <label>Title</label>
              <input value={title} onChange={(e) => setTitle(e.target.value)} />
            </div>
            <div style={{ flex: "0 0 140px" }}>
              <label>Type</label>
              <select value={type} onChange={(e) => setType(e.target.value as ChartType)}>
                {TYPES.map((t) => (
                  <option key={t} value={t}>
                    {t}
                  </option>
                ))}
              </select>
            </div>
          </div>
          <label>SQL (SELECT only — runs read-only)</label>
          <textarea
            value={sql}
            onChange={(e) => setSql(e.target.value)}
            placeholder="SELECT date_trunc('day', created_at)::date AS day, count(*) AS signups FROM users GROUP BY 1 ORDER BY 1"
          />
          {showAxes && (
            <div className="row">
              <div>
                <label>x column</label>
                <input value={x} onChange={(e) => setX(e.target.value)} placeholder="day" />
              </div>
              <div>
                <label>y column</label>
                <input value={y} onChange={(e) => setY(e.target.value)} placeholder="signups" />
              </div>
              <div>
                <label>series column (optional)</label>
                <input
                  value={series}
                  onChange={(e) => setSeries(e.target.value)}
                  placeholder="currency"
                />
              </div>
            </div>
          )}
          <div style={{ marginTop: 14 }}>
            <button className="btn" disabled={busy || !title || !sql} onClick={submit}>
              {busy ? "Saving…" : "Save chart"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
