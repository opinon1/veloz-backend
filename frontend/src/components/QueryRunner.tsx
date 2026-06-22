import { useState } from "react";
import { runQuery, ApiError } from "../api";
import type { QueryResult } from "../types";
import ChartView from "./ChartView";
import NewChartForm from "./NewChartForm";

function toCsv(res: QueryResult): string {
  const esc = (v: unknown) => {
    const s = v === null || v === undefined ? "" : typeof v === "object" ? JSON.stringify(v) : String(v);
    return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
  };
  const head = res.columns.map(esc).join(",");
  const body = res.rows.map((r) => res.columns.map((c) => esc(r[c])).join(",")).join("\n");
  return head + "\n" + body;
}

export default function QueryRunner() {
  const [sql, setSql] = useState("");
  const [res, setRes] = useState<QueryResult | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const run = async () => {
    setBusy(true);
    setErr(null);
    try {
      setRes(await runQuery(sql));
    } catch (e) {
      setRes(null);
      setErr(e instanceof ApiError ? e.message : "query failed");
    } finally {
      setBusy(false);
    }
  };

  const exportCsv = () => {
    if (!res) return;
    const blob = new Blob([toCsv(res)], { type: "text/csv" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "veloz-query.csv";
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <section>
      <div className="panel">
        <h2>Ad-hoc query</h2>
        <p className="hint">
          Read-only. Single <code>SELECT</code>, capped at 5000 rows, 10s timeout.
        </p>
        <textarea
          value={sql}
          onChange={(e) => setSql(e.target.value)}
          placeholder="SELECT status, count(*) FROM payments GROUP BY 1 ORDER BY 2 DESC"
          onKeyDown={(e) => {
            if ((e.metaKey || e.ctrlKey) && e.key === "Enter") run();
          }}
        />
        <div className="row" style={{ marginTop: 12 }}>
          <button className="btn" disabled={busy || !sql.trim()} onClick={run}>
            {busy ? "Running…" : "Run (⌘/Ctrl+Enter)"}
          </button>
          <button className="btn ghost" disabled={!res} onClick={exportCsv}>
            Export CSV
          </button>
        </div>
      </div>

      {err && (
        <div className="panel">
          <div className="err-text">{err}</div>
        </div>
      )}
      {res && (
        <div className="panel">
          <ChartView type="table" config={{}} result={res} />
        </div>
      )}

      {sql.trim() && <NewChartForm initialSql={sql} onCreated={() => {}} />}
    </section>
  );
}
