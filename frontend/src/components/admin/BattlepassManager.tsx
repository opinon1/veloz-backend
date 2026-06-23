import { useCallback, useEffect, useState } from "react";
import { adminCall } from "../../api";
import { useToast } from "../Toast";
import { JsonArea, OutputBox, cell, errMsg, pretty, useActionResult } from "./common";

type Row = Record<string, unknown>;

const SEASON_TEMPLATE = {
  name: "Season 1",
  description: "Launch season",
  starts_at: "2026-04-22T00:00:00Z",
  ends_at: "2026-06-22T00:00:00Z",
  premium_cost: 100,
  premium_currency: "high",
  metadata: {},
};
const TIER_TEMPLATE = {
  tier: 1,
  xp_required: 100,
  free_reward: [{ type: "currency", currency: "soft", amount: 50 }],
  premium_reward: [{ type: "currency", currency: "high", amount: 10 }],
};

export default function BattlepassManager() {
  const toast = useToast();
  const { result, setResult } = useActionResult();
  const [seasons, setSeasons] = useState<Row[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [tiers, setTiers] = useState<Row[]>([]);
  // editor: {scope, mode, id?}
  const [editor, setEditor] = useState<{ scope: "season" | "tier"; mode: "create" | "edit"; id?: string } | null>(null);
  const [draft, setDraft] = useState("");

  const loadSeasons = useCallback(async () => {
    try {
      const d = await adminCall<Row[]>("GET", "/admin/battlepass/seasons");
      setSeasons(Array.isArray(d) ? d : []);
    } catch (e) {
      toast(errMsg(e), "err");
    }
  }, [toast]);

  const loadTiers = useCallback(async (sid: string) => {
    try {
      const d = await adminCall<Row[]>("GET", `/admin/battlepass/seasons/${sid}/tiers`);
      setTiers(Array.isArray(d) ? d : []);
    } catch (e) {
      toast(errMsg(e), "err");
    }
  }, [toast]);

  useEffect(() => { loadSeasons(); }, [loadSeasons]);
  useEffect(() => { if (selected) loadTiers(selected); }, [selected, loadTiers]);

  const run = async (label: string, method: string, path: string, body?: unknown) => {
    try {
      const res = await adminCall(method, path, body);
      setResult({ ok: true, label: `${method} ${path}`, body: res ? pretty(res) : "(ok)" });
      toast(label, "ok");
      return true;
    } catch (e) {
      setResult({ ok: false, label: `${method} ${path}`, body: errMsg(e) });
      toast(errMsg(e), "err");
      return false;
    }
  };

  const openSeason = (mode: "create" | "edit", row?: Row) => {
    setDraft(mode === "create" ? pretty(SEASON_TEMPLATE) : pretty(strip(row!)));
    setEditor({ scope: "season", mode, id: row ? String(row.id) : undefined });
  };
  const openTier = (mode: "create" | "edit", row?: Row) => {
    setDraft(mode === "create" ? pretty(TIER_TEMPLATE) : pretty(strip(row!)));
    setEditor({ scope: "tier", mode, id: row ? String(row.id) : undefined });
  };

  const submit = async () => {
    let body: unknown;
    try { body = JSON.parse(draft); } catch { return toast("Invalid JSON", "err"); }
    if (!editor) return;
    let ok = false;
    if (editor.scope === "season") {
      ok = editor.mode === "create"
        ? await run("Season created", "POST", "/admin/battlepass/seasons", body)
        : await run("Season updated", "PATCH", `/admin/battlepass/seasons/${editor.id}`, body);
      if (ok) loadSeasons();
    } else {
      ok = editor.mode === "create"
        ? await run("Tier created", "POST", `/admin/battlepass/seasons/${selected}/tiers`, body)
        : await run("Tier updated", "PATCH", `/admin/battlepass/tiers/${editor.id}`, body);
      if (ok && selected) loadTiers(selected);
    }
    if (ok) setEditor(null);
  };

  const delSeason = async (id: string) => {
    if (!confirm(`Delete season ${id} (and its tiers)?`)) return;
    if (await run("Season deleted", "DELETE", `/admin/battlepass/seasons/${id}`)) {
      if (selected === id) setSelected(null);
      loadSeasons();
    }
  };
  const delTier = async (id: string) => {
    if (!confirm(`Delete tier ${id}?`)) return;
    if (await run("Tier deleted", "DELETE", `/admin/battlepass/tiers/${id}`) && selected) loadTiers(selected);
  };

  return (
    <div>
      <div className="panel">
        <div style={{ display: "flex", alignItems: "center" }}>
          <h2 style={{ margin: 0 }}>Seasons</h2>
          <div style={{ marginLeft: "auto", display: "flex", gap: 8 }}>
            <button className="btn ghost sm" onClick={loadSeasons}>↻</button>
            <button className="btn sm" onClick={() => openSeason("create")}>+ New season</button>
          </div>
        </div>
        <div className="tablewrap" style={{ marginTop: 12 }}>
          <table>
            <thead><tr><th>name</th><th>starts_at</th><th>ends_at</th><th>premium_cost</th><th>actions</th></tr></thead>
            <tbody>
              {seasons.map((s) => (
                <tr key={String(s.id)} style={{ background: selected === s.id ? "var(--bg)" : undefined }}>
                  <td>{cell(s.name)}</td>
                  <td>{cell(s.starts_at)}</td>
                  <td>{cell(s.ends_at)}</td>
                  <td>{cell(s.premium_cost)} {cell(s.premium_currency)}</td>
                  <td style={{ whiteSpace: "nowrap" }}>
                    <button className="btn ghost sm" onClick={() => setSelected(String(s.id))}>tiers</button>{" "}
                    <button className="btn ghost sm" onClick={() => openSeason("edit", s)}>edit</button>{" "}
                    <button className="btn danger sm" onClick={() => delSeason(String(s.id))}>del</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>

      {selected && (
        <div className="panel">
          <div style={{ display: "flex", alignItems: "center" }}>
            <h2 style={{ margin: 0 }}>Tiers · season {selected.slice(0, 8)}…</h2>
            <div style={{ marginLeft: "auto", display: "flex", gap: 8 }}>
              <button className="btn ghost sm" onClick={() => setSelected(null)}>close</button>
              <button className="btn sm" onClick={() => openTier("create")}>+ New tier</button>
            </div>
          </div>
          <div className="tablewrap" style={{ marginTop: 12 }}>
            <table>
              <thead><tr><th>tier</th><th>xp_required</th><th>free_reward</th><th>premium_reward</th><th>actions</th></tr></thead>
              <tbody>
                {tiers.map((t) => (
                  <tr key={String(t.id)}>
                    <td>{cell(t.tier)}</td>
                    <td>{cell(t.xp_required)}</td>
                    <td style={{ maxWidth: 200, overflow: "hidden", textOverflow: "ellipsis" }}>{cell(t.free_reward)}</td>
                    <td style={{ maxWidth: 200, overflow: "hidden", textOverflow: "ellipsis" }}>{cell(t.premium_reward)}</td>
                    <td style={{ whiteSpace: "nowrap" }}>
                      <button className="btn ghost sm" onClick={() => openTier("edit", t)}>edit</button>{" "}
                      <button className="btn danger sm" onClick={() => delTier(String(t.id))}>del</button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {editor && (
        <div className="panel">
          <h2>{editor.mode === "create" ? `New ${editor.scope}` : `Edit ${editor.scope} ${editor.id?.slice(0, 8)}…`}</h2>
          <JsonArea value={draft} onChange={setDraft} />
          <div className="row" style={{ marginTop: 12 }}>
            <button className="btn" onClick={submit}>{editor.mode === "create" ? "Create" : "Save"}</button>
            <button className="btn ghost" onClick={() => setEditor(null)}>Cancel</button>
          </div>
        </div>
      )}

      <OutputBox result={result} />
    </div>
  );
}

function strip(row: Row): Row {
  const out: Row = {};
  for (const k of Object.keys(row)) {
    if (!["id", "season_id", "created_at", "updated_at"].includes(k)) out[k] = row[k];
  }
  return out;
}
