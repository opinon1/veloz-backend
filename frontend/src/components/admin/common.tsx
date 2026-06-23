import { useState } from "react";
import { ApiError } from "../../api";

/** Shared result/error display for admin actions. */
export interface ActionResult {
  ok: boolean;
  label: string;
  body: string;
}

export function OutputBox({ result }: { result: ActionResult | null }) {
  if (!result) return null;
  return (
    <div className="panel">
      <h2 className={result.ok ? "ok-text" : "err-text"}>
        {result.ok ? "OK · " : "Error · "}
        {result.label}
      </h2>
      <pre className="out">{result.body}</pre>
    </div>
  );
}

export function errMsg(e: unknown): string {
  return e instanceof ApiError ? e.message : e instanceof Error ? e.message : String(e);
}

/** Controlled JSON textarea with live parse validity. */
export function JsonArea({
  value,
  onChange,
  rows = 12,
}: {
  value: string;
  onChange: (v: string) => void;
  rows?: number;
}) {
  let valid = true;
  if (value.trim()) {
    try {
      JSON.parse(value);
    } catch {
      valid = false;
    }
  }
  return (
    <div>
      <textarea
        spellCheck={false}
        style={{ minHeight: rows * 18, borderColor: valid ? undefined : "var(--danger)" }}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
      {!valid && <div className="err-text" style={{ fontSize: 12 }}>invalid JSON</div>}
    </div>
  );
}

/** Pretty-print a value as JSON for an editor. */
export function pretty(v: unknown): string {
  return JSON.stringify(v, null, 2);
}

/** Render a cell value for a table. */
export function cell(v: unknown): string {
  if (v === null || v === undefined) return "";
  if (typeof v === "object") return JSON.stringify(v);
  return String(v);
}

/** A small hook holding the latest action result + a runner that toasts. */
export function useActionResult() {
  const [result, setResult] = useState<ActionResult | null>(null);
  return { result, setResult };
}
