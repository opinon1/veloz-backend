import { useCallback, useEffect, useState } from "react";
import { adminCall } from "../../api";
import { useToast } from "../Toast";
import { JsonArea, OutputBox, cell, errMsg, useActionResult } from "./common";

type User = Record<string, unknown>;

export default function UserManager() {
  const toast = useToast();
  const { result, setResult } = useActionResult();
  const [search, setSearch] = useState("");
  const [users, setUsers] = useState<User[]>([]);
  const [loading, setLoading] = useState(false);

  // shared selected target
  const [uid, setUid] = useState("");
  // grant
  const [gCur, setGCur] = useState("high");
  const [gAmt, setGAmt] = useState("");
  const [gReason, setGReason] = useState("admin_grant");
  // role
  const [role, setRole] = useState("user");
  // profile (JSON — many optional fields)
  const [profile, setProfile] = useState('{\n  "price_multiplier": 0.8,\n  "main_highscore": 99999\n}');

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const q = new URLSearchParams({ limit: "100" });
      if (search.trim()) q.set("search", search.trim());
      const data = await adminCall<User[]>("GET", `/admin/users?${q.toString()}`);
      setUsers(Array.isArray(data) ? data : []);
    } catch (e) {
      toast(errMsg(e), "err");
    } finally {
      setLoading(false);
    }
  }, [search, toast]);

  useEffect(() => {
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const run = async (label: string, method: string, path: string, body?: unknown) => {
    try {
      const res = await adminCall(method, path, body);
      setResult({ ok: true, label: `${method} ${path}`, body: JSON.stringify(res, null, 2) });
      toast(label + " ✓", "ok");
      load();
    } catch (e) {
      setResult({ ok: false, label: `${method} ${path}`, body: errMsg(e) });
      toast(errMsg(e), "err");
    }
  };

  const grant = () =>
    run("Grant", "POST", `/admin/users/${uid}/grant`, {
      currency: gCur,
      amount: parseInt(gAmt, 10),
      ...(gReason ? { reason: gReason } : {}),
    });
  const setUserRole = () => run("Role", "PATCH", `/admin/users/${uid}/role`, { role });
  const setProfileFields = () => {
    let body: unknown;
    try {
      body = JSON.parse(profile);
    } catch {
      return toast("Profile JSON invalid", "err");
    }
    run("Profile", "PATCH", `/admin/users/${uid}/profile`, body);
  };

  const cols = ["username", "email", "role", "is_active", "account_level", "high", "soft", "energy"];

  return (
    <div>
      <div className="panel">
        <div style={{ display: "flex", gap: 10 }}>
          <input
            placeholder="Search username / email…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && load()}
          />
          <button className="btn" style={{ flex: "0 0 auto" }} onClick={load}>Search</button>
        </div>
        {loading ? (
          <div className="hint" style={{ marginTop: 12 }}>Loading…</div>
        ) : (
          <div className="tablewrap" style={{ marginTop: 12 }}>
            <table>
              <thead>
                <tr>
                  {cols.map((c) => <th key={c}>{c}</th>)}
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {users.map((u) => (
                  <tr key={String(u.id)}>
                    {cols.map((c) => <td key={c}>{cell(u[c])}</td>)}
                    <td>
                      <button className="btn ghost sm" onClick={() => setUid(String(u.id))}>select</button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      <div className="panel">
        <h2>Target user</h2>
        <p className="hint">Set by "select" above, or paste a user id. Used by all actions below.</p>
        <input value={uid} onChange={(e) => setUid(e.target.value)} placeholder="user id (uuid)" />
      </div>

      <div className="panel">
        <h2>Grant / deduct currency</h2>
        <p className="hint"><code>POST /admin/users/:id/grant</code> — negative deducts.</p>
        <div className="row">
          <div style={{ flex: "0 0 130px" }}>
            <label>Currency</label>
            <select value={gCur} onChange={(e) => setGCur(e.target.value)}>
              <option>high</option><option>soft</option><option>energy</option>
            </select>
          </div>
          <div style={{ flex: "0 0 140px" }}>
            <label>Amount</label>
            <input type="number" value={gAmt} onChange={(e) => setGAmt(e.target.value)} />
          </div>
          <div>
            <label>Reason</label>
            <input value={gReason} onChange={(e) => setGReason(e.target.value)} />
          </div>
        </div>
        <div style={{ marginTop: 12 }}>
          <button className="btn" disabled={!uid || !gAmt} onClick={grant}>Apply grant</button>
        </div>
      </div>

      <div className="panel">
        <h2>Set role</h2>
        <p className="hint"><code>PATCH /admin/users/:id/role</code></p>
        <div className="row">
          <div style={{ flex: "0 0 160px" }}>
            <select value={role} onChange={(e) => setRole(e.target.value)}>
              <option>user</option><option>admin</option>
            </select>
          </div>
          <div style={{ flex: "0 0 auto" }}>
            <button className="btn" disabled={!uid} onClick={setUserRole}>Update role</button>
          </div>
        </div>
      </div>

      <div className="panel">
        <h2>Override profile</h2>
        <p className="hint">
          <code>PATCH /admin/users/:id/profile</code> — any of: account_level, total_xp,
          price_multiplier, main_highscore, avatar_url, frame_url.
        </p>
        <JsonArea value={profile} onChange={setProfile} rows={6} />
        <div style={{ marginTop: 12 }}>
          <button className="btn" disabled={!uid} onClick={setProfileFields}>Apply profile</button>
        </div>
      </div>

      <OutputBox result={result} />
    </div>
  );
}
