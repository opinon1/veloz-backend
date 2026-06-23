import { useCallback, useEffect, useState } from "react";
import { adminCall } from "../../api";
import type { ResourceDef } from "../../admin/resources";
import { READONLY_FIELDS } from "../../admin/resources";
import { useToast } from "../Toast";
import { JsonArea, OutputBox, cell, errMsg, pretty, useActionResult } from "./common";

type Row = Record<string, unknown>;

export default function ResourceManager({ def }: { def: ResourceDef }) {
  const toast = useToast();
  const { result, setResult } = useActionResult();
  const [rows, setRows] = useState<Row[]>([]);
  const [loading, setLoading] = useState(true);
  // editor: null = closed; {mode:'create'} or {mode:'edit', id}
  const [editor, setEditor] = useState<{ mode: "create" | "edit"; id?: string } | null>(null);
  const [draft, setDraft] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const data = await adminCall<Row[]>("GET", def.path);
      setRows(Array.isArray(data) ? data : []);
    } catch (e) {
      toast(errMsg(e), "err");
    } finally {
      setLoading(false);
    }
  }, [def.path, toast]);

  useEffect(() => {
    setEditor(null);
    setResult(null);
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [def.key]);

  const openCreate = () => {
    setDraft(pretty(def.template));
    setEditor({ mode: "create" });
  };
  const openEdit = (row: Row) => {
    const editable: Row = {};
    for (const k of Object.keys(row)) {
      if (!READONLY_FIELDS.includes(k)) editable[k] = row[k];
    }
    setDraft(pretty(editable));
    setEditor({ mode: "edit", id: String(row.id) });
  };

  const submit = async () => {
    let body: unknown;
    try {
      body = JSON.parse(draft);
    } catch {
      toast("Body is not valid JSON", "err");
      return;
    }
    const isCreate = editor?.mode === "create";
    const method = isCreate ? "POST" : "PATCH";
    const path = isCreate ? def.path : `${def.path}/${editor?.id}`;
    try {
      const res = await adminCall(method, path, body);
      setResult({ ok: true, label: `${method} ${path}`, body: pretty(res) });
      toast(isCreate ? "Created" : "Updated", "ok");
      setEditor(null);
      load();
    } catch (e) {
      setResult({ ok: false, label: `${method} ${path}`, body: errMsg(e) });
      toast(errMsg(e), "err");
    }
  };

  const remove = async (id: string) => {
    if (!confirm(`Delete ${def.label.replace(/s$/, "")} ${id}?`)) return;
    const path = `${def.path}/${id}`;
    try {
      await adminCall("DELETE", path);
      setResult({ ok: true, label: `DELETE ${path}`, body: "(deleted)" });
      toast("Deleted", "ok");
      load();
    } catch (e) {
      setResult({ ok: false, label: `DELETE ${path}`, body: errMsg(e) });
      toast(errMsg(e), "err");
    }
  };

  return (
    <div>
      <div className="panel">
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <h2 style={{ margin: 0 }}>{def.label}</h2>
          <span className="hint" style={{ margin: 0 }}>{rows.length} rows</span>
          <div style={{ marginLeft: "auto", display: "flex", gap: 8 }}>
            <button className="btn ghost sm" onClick={load}>↻ Refresh</button>
            {!def.noCreate && <button className="btn sm" onClick={openCreate}>+ New</button>}
          </div>
        </div>

        {loading ? (
          <div className="hint" style={{ marginTop: 12 }}>Loading…</div>
        ) : (
          <div className="tablewrap" style={{ marginTop: 12 }}>
            <table>
              <thead>
                <tr>
                  {def.cols.map((c) => (
                    <th key={c}>{c}</th>
                  ))}
                  <th>actions</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r) => (
                  <tr key={String(r.id)}>
                    {def.cols.map((c) => (
                      <td key={c} title={cell(r[c])} style={{ maxWidth: 220, overflow: "hidden", textOverflow: "ellipsis" }}>
                        {cell(r[c])}
                      </td>
                    ))}
                    <td style={{ whiteSpace: "nowrap" }}>
                      <button className="btn ghost sm" onClick={() => openEdit(r)}>edit</button>{" "}
                      {!def.noDelete && (
                        <button className="btn danger sm" onClick={() => remove(String(r.id))}>del</button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {editor && (
        <div className="panel">
          <h2>{editor.mode === "create" ? `New ${def.label}` : `Edit ${editor.id}`}</h2>
          <p className="hint">JSON body (validated server-side; errors shown below).</p>
          <JsonArea value={draft} onChange={setDraft} />
          <div className="row" style={{ marginTop: 12 }}>
            <button className="btn" onClick={submit}>
              {editor.mode === "create" ? "Create" : "Save"}
            </button>
            <button className="btn ghost" onClick={() => setEditor(null)}>Cancel</button>
          </div>
        </div>
      )}

      <OutputBox result={result} />
    </div>
  );
}
