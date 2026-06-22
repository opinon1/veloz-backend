import { Line, Bar, Pie } from "react-chartjs-2";
import { PALETTE } from "../chartSetup";
import type { ChartConfig, ChartType, QueryResult } from "../types";

function num(v: unknown): number {
  const n = typeof v === "number" ? v : parseFloat(String(v));
  return isNaN(n) ? 0 : n;
}
function fmt(v: unknown): string {
  if (v === null || v === undefined) return "";
  if (typeof v === "object") return JSON.stringify(v);
  return String(v);
}

const axisOpts = {
  responsive: true,
  plugins: { legend: { labels: { color: "#8b93a1" } } },
  scales: {
    x: { ticks: { color: "#8b93a1" }, grid: { color: "#262b36" } },
    y: { ticks: { color: "#8b93a1" }, grid: { color: "#262b36" } },
  },
};

function buildSeries(type: ChartType, cfg: ChartConfig, res: QueryResult) {
  const rows = res.rows;
  const cols = res.columns;
  const x = cfg.x || cols[0];
  const y = cfg.y || cols.find((c) => c !== x) || cols[1] || cols[0];
  const series = cfg.series;

  if (type === "pie") {
    return {
      labels: rows.map((r) => String(r[x])),
      datasets: [{ data: rows.map((r) => num(r[y])), backgroundColor: PALETTE }],
    };
  }
  if (series) {
    const xs = [...new Set(rows.map((r) => String(r[x])))];
    const groups = [...new Set(rows.map((r) => String(r[series])))];
    return {
      labels: xs,
      datasets: groups.map((g, i) => ({
        label: g,
        data: xs.map((xv) => {
          const m = rows.find((r) => String(r[x]) === xv && String(r[series]) === g);
          return m ? num(m[y]) : null;
        }),
        borderColor: PALETTE[i % PALETTE.length],
        backgroundColor: PALETTE[i % PALETTE.length],
        tension: 0.25,
      })),
    };
  }
  return {
    labels: rows.map((r) => String(r[x])),
    datasets: [
      {
        label: y,
        data: rows.map((r) => num(r[y])),
        borderColor: PALETTE[0],
        backgroundColor: PALETTE[0],
        tension: 0.25,
      },
    ],
  };
}

export default function ChartView({
  type,
  config,
  result,
}: {
  type: ChartType;
  config: ChartConfig;
  result: QueryResult;
}) {
  if (type === "stat") {
    const r = result.rows[0] || {};
    return (
      <div className="stat-row">
        {result.columns.map((c) => (
          <div className="stat" key={c}>
            <div className="v">{fmt(r[c])}</div>
            <div className="k">{c.replace(/_/g, " ")}</div>
          </div>
        ))}
      </div>
    );
  }

  if (type === "line" || type === "bar" || type === "pie") {
    const data = buildSeries(type, config, result);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const d = data as any;
    if (type === "line") return <Line data={d} options={axisOpts} />;
    if (type === "bar") return <Bar data={d} options={axisOpts} />;
    return <Pie data={d} options={{ responsive: true, plugins: { legend: { labels: { color: "#8b93a1" } } } }} />;
  }

  // table (default)
  if (!result.rows.length) return <div className="hint">no rows</div>;
  return (
    <div>
      <div className="tablewrap">
        <table>
          <thead>
            <tr>
              {result.columns.map((c) => (
                <th key={c}>{c}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {result.rows.map((r, i) => (
              <tr key={i}>
                {result.columns.map((c) => (
                  <td key={c}>{fmt(r[c])}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="hint" style={{ marginTop: 6 }}>
        {result.row_count} rows
      </div>
    </div>
  );
}
