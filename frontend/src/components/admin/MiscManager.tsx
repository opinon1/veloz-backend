import { useState } from "react";
import { adminCall } from "../../api";
import { useToast } from "../Toast";
import { OutputBox, errMsg, pretty, useActionResult } from "./common";

export default function MiscManager() {
  const toast = useToast();
  const { result, setResult } = useActionResult();
  const [method, setMethod] = useState("GET");
  const [path, setPath] = useState("");
  const [body, setBody] = useState("");

  const run = async (label: string, m: string, p: string, b?: unknown) => {
    try {
      const res = await adminCall(m, p, b);
      setResult({ ok: true, label: `${m} ${p}`, body: res ? pretty(res) : "(no content)" });
      toast(label, "ok");
    } catch (e) {
      setResult({ ok: false, label: `${m} ${p}`, body: errMsg(e) });
      toast(errMsg(e), "err");
    }
  };

  const backfill = () => run("Backfill done", "POST", "/admin/signup-defaults/backfill");

  const raw = () => {
    let b: unknown;
    const t = body.trim();
    if (t) {
      try { b = JSON.parse(t); } catch { return toast("Body is not valid JSON", "err"); }
    }
    run("Sent", method, path, b);
  };

  return (
    <div>
      <div className="panel">
        <h2>Backfill signup defaults</h2>
        <p className="hint"><code>POST /admin/signup-defaults/backfill</code> — idempotent; applies every default catalog row to all users.</p>
        <button className="btn" onClick={backfill}>Run backfill</button>
      </div>

      <div className="panel">
        <h2>Raw admin call</h2>
        <p className="hint">Escape hatch for any <code>/admin/*</code> (or any) endpoint. Body JSON, blank for GET/DELETE.</p>
        <div className="row">
          <div style={{ flex: "0 0 120px" }}>
            <label>Method</label>
            <select value={method} onChange={(e) => setMethod(e.target.value)}>
              <option>GET</option><option>POST</option><option>PATCH</option><option>PUT</option><option>DELETE</option>
            </select>
          </div>
          <div>
            <label>Path</label>
            <input value={path} onChange={(e) => setPath(e.target.value)} placeholder="/admin/users?limit=50" />
          </div>
        </div>
        <label>JSON body</label>
        <textarea value={body} onChange={(e) => setBody(e.target.value)} placeholder='{ "key": "value" }' />
        <div style={{ marginTop: 12 }}>
          <button className="btn" disabled={!path} onClick={raw}>Send</button>
        </div>
      </div>

      <OutputBox result={result} />
    </div>
  );
}
