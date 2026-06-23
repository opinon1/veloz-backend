import { useCallback, useEffect, useState } from "react";
import { adminCall } from "../../api";
import { useToast } from "../Toast";
import { JsonArea, OutputBox, cell, errMsg, pretty, useActionResult } from "./common";

type Row = Record<string, unknown>;

const WHEEL_TEMPLATE = {
  items: [
    { reward: [{ type: "currency", currency: "soft", amount: 100 }], weight: 5 },
    { reward: [{ type: "currency", currency: "high", amount: 10 }], weight: 1 },
  ],
};

export default function PrizeWheelManager() {
  const toast = useToast();
  const { result, setResult } = useActionResult();
  const [items, setItems] = useState<Row[]>([]);
  const [draft, setDraft] = useState(pretty(WHEEL_TEMPLATE));

  const load = useCallback(async () => {
    try {
      const d = await adminCall<Row[]>("GET", "/admin/prize-wheel");
      setItems(Array.isArray(d) ? d : []);
      if (Array.isArray(d) && d.length) {
        setDraft(pretty({ items: d.map((i) => ({ reward: i.reward, weight: i.weight })) }));
      }
    } catch (e) {
      toast(errMsg(e), "err");
    }
  }, [toast]);

  useEffect(() => { load(); }, [load]);

  const run = async (label: string, method: string, path: string, body?: unknown) => {
    try {
      const res = await adminCall(method, path, body);
      setResult({ ok: true, label: `${method} ${path}`, body: res ? pretty(res) : "(ok)" });
      toast(label, "ok");
      load();
    } catch (e) {
      setResult({ ok: false, label: `${method} ${path}`, body: errMsg(e) });
      toast(errMsg(e), "err");
    }
  };

  const replace = () => {
    let body: unknown;
    try { body = JSON.parse(draft); } catch { return toast("Invalid JSON", "err"); }
    run("Wheel replaced", "PUT", "/admin/prize-wheel", body);
  };
  const clear = () => {
    if (confirm("Empty the entire wheel?")) run("Wheel cleared", "DELETE", "/admin/prize-wheel");
  };
  const clearCooldown = () => run("Cooldown cleared", "DELETE", "/admin/prize-wheel/cooldown");

  return (
    <div>
      <div className="panel">
        <div style={{ display: "flex", alignItems: "center" }}>
          <h2 style={{ margin: 0 }}>Current wheel</h2>
          <div style={{ marginLeft: "auto", display: "flex", gap: 8 }}>
            <button className="btn ghost sm" onClick={load}>↻</button>
            <button className="btn ghost sm" onClick={clearCooldown}>clear my cooldown</button>
            <button className="btn danger sm" onClick={clear}>empty wheel</button>
          </div>
        </div>
        <div className="tablewrap" style={{ marginTop: 12 }}>
          <table>
            <thead><tr><th>position</th><th>weight</th><th>reward</th></tr></thead>
            <tbody>
              {items.map((i) => (
                <tr key={String(i.id ?? i.position)}>
                  <td>{cell(i.position)}</td>
                  <td>{cell(i.weight)}</td>
                  <td>{cell(i.reward)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      <div className="panel">
        <h2>Replace wheel</h2>
        <p className="hint"><code>PUT /admin/prize-wheel</code> — atomic full replace. Each item: reward (grant array) + weight ≥ 1.</p>
        <JsonArea value={draft} onChange={setDraft} />
        <div className="row" style={{ marginTop: 12 }}>
          <button className="btn" onClick={replace}>Replace wheel</button>
        </div>
      </div>

      <OutputBox result={result} />
    </div>
  );
}
