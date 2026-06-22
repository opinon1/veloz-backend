import { useState } from "react";
import { adminCall, ApiError } from "../api";
import { useToast } from "./Toast";

interface Result {
  ok: boolean;
  label: string;
  body: string;
}

export default function AdminActions() {
  const toast = useToast();
  const [result, setResult] = useState<Result | null>(null);

  // grant
  const [gUser, setGUser] = useState("");
  const [gCur, setGCur] = useState("high");
  const [gAmt, setGAmt] = useState("");
  const [gReason, setGReason] = useState("");
  // role
  const [rUser, setRUser] = useState("");
  const [rRole, setRRole] = useState("user");
  // raw
  const [rawMethod, setRawMethod] = useState("GET");
  const [rawPath, setRawPath] = useState("");
  const [rawBody, setRawBody] = useState("");

  const run = async (label: string, method: string, path: string, body?: unknown) => {
    try {
      const res = await adminCall(method, path, body);
      setResult({ ok: true, label: `${method} ${path}`, body: JSON.stringify(res, null, 2) || "(no content)" });
      toast(label + " ✓", "ok");
    } catch (e) {
      const msg = e instanceof ApiError ? e.message : String(e);
      setResult({ ok: false, label: `${method} ${path}`, body: msg });
      toast(msg, "err");
    }
  };

  const grant = () =>
    run("Grant", "POST", `/admin/users/${gUser}/grant`, {
      currency: gCur,
      amount: parseInt(gAmt, 10),
      ...(gReason ? { reason: gReason } : {}),
    });

  const setRole = () =>
    run("Role update", "PATCH", `/admin/users/${rUser}/role`, { role: rRole });

  const backfill = () =>
    run("Backfill", "POST", "/admin/signup-defaults/backfill");

  const raw = () => {
    let body: unknown;
    const txt = rawBody.trim();
    if (txt) {
      try {
        body = JSON.parse(txt);
      } catch {
        toast("Body is not valid JSON", "err");
        return;
      }
    }
    run("Raw call", rawMethod, rawPath, body);
  };

  return (
    <section>
      <div className="panel">
        <h2>Grant / deduct currency</h2>
        <p className="hint">
          <code>POST /admin/users/:id/grant</code> — negative amount deducts.
        </p>
        <div className="row">
          <div>
            <label>User ID</label>
            <input value={gUser} onChange={(e) => setGUser(e.target.value)} />
          </div>
          <div style={{ flex: "0 0 130px" }}>
            <label>Currency</label>
            <select value={gCur} onChange={(e) => setGCur(e.target.value)}>
              <option>high</option>
              <option>soft</option>
              <option>energy</option>
            </select>
          </div>
          <div style={{ flex: "0 0 130px" }}>
            <label>Amount</label>
            <input type="number" value={gAmt} onChange={(e) => setGAmt(e.target.value)} />
          </div>
          <div>
            <label>Reason</label>
            <input value={gReason} onChange={(e) => setGReason(e.target.value)} placeholder="admin_grant" />
          </div>
        </div>
        <div style={{ marginTop: 12 }}>
          <button className="btn" disabled={!gUser || !gAmt} onClick={grant}>
            Apply grant
          </button>
        </div>
      </div>

      <div className="panel">
        <h2>Set user role</h2>
        <p className="hint">
          <code>PATCH /admin/users/:id/role</code>
        </p>
        <div className="row">
          <div>
            <label>User ID</label>
            <input value={rUser} onChange={(e) => setRUser(e.target.value)} />
          </div>
          <div style={{ flex: "0 0 160px" }}>
            <label>Role</label>
            <select value={rRole} onChange={(e) => setRRole(e.target.value)}>
              <option>user</option>
              <option>admin</option>
            </select>
          </div>
        </div>
        <div style={{ marginTop: 12 }}>
          <button className="btn" disabled={!rUser} onClick={setRole}>
            Update role
          </button>
        </div>
      </div>

      <div className="panel">
        <h2>Backfill signup defaults</h2>
        <p className="hint">
          <code>POST /admin/signup-defaults/backfill</code> — idempotent, applies default catalog to all
          users.
        </p>
        <button className="btn" onClick={backfill}>
          Run backfill
        </button>
      </div>

      <div className="panel">
        <h2>Raw admin call</h2>
        <p className="hint">
          Escape hatch for any <code>/admin/*</code> endpoint. Body is JSON (blank for GET/DELETE).
        </p>
        <div className="row">
          <div style={{ flex: "0 0 120px" }}>
            <label>Method</label>
            <select value={rawMethod} onChange={(e) => setRawMethod(e.target.value)}>
              <option>GET</option>
              <option>POST</option>
              <option>PATCH</option>
              <option>PUT</option>
              <option>DELETE</option>
            </select>
          </div>
          <div>
            <label>Path</label>
            <input value={rawPath} onChange={(e) => setRawPath(e.target.value)} placeholder="/admin/store" />
          </div>
        </div>
        <label>JSON body</label>
        <textarea value={rawBody} onChange={(e) => setRawBody(e.target.value)} placeholder='{ "name": "..." }' />
        <div style={{ marginTop: 12 }}>
          <button className="btn" disabled={!rawPath} onClick={raw}>
            Send
          </button>
        </div>
      </div>

      {result && (
        <div className="panel">
          <h2 className={result.ok ? "ok-text" : "err-text"}>
            {result.ok ? "OK · " : "Error · "}
            {result.label}
          </h2>
          <pre className="out">{result.body}</pre>
        </div>
      )}
    </section>
  );
}
