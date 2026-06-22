import type { ChartDef, NewChart, QueryResult } from "./types";

const BASE = (import.meta.env.VITE_API_BASE as string | undefined) ?? "";
const TOKEN_KEY = "veloz_admin_token";

export function getToken(): string {
  return localStorage.getItem(TOKEN_KEY) ?? "";
}
export function setToken(t: string) {
  localStorage.setItem(TOKEN_KEY, t);
}
export function clearToken() {
  localStorage.removeItem(TOKEN_KEY);
}

/** Thrown on any non-2xx response; `status` lets callers special-case 401/403. */
export class ApiError extends Error {
  status: number;
  constructor(message: string, status: number) {
    super(message);
    this.status = status;
  }
}

interface ReqOpts {
  method?: string;
  body?: unknown;
  auth?: boolean; // attach Bearer token (default true)
}

async function request<T>(path: string, opts: ReqOpts = {}): Promise<T> {
  const { method = "GET", body, auth = true } = opts;
  const headers: Record<string, string> = {};
  if (auth) headers["Authorization"] = `Bearer ${getToken()}`;
  if (body !== undefined) headers["Content-Type"] = "application/json";

  const res = await fetch(BASE + path, {
    method,
    headers,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });

  const text = await res.text();
  let data: unknown = null;
  try {
    data = text ? JSON.parse(text) : null;
  } catch {
    data = text;
  }

  if (!res.ok) {
    const msg =
      typeof data === "string"
        ? data
        : (data as { message?: string; error?: string })?.message ||
          (data as { error?: string })?.error ||
          `HTTP ${res.status}`;
    throw new ApiError(msg, res.status);
  }
  return data as T;
}

// ───── auth ─────
export async function signin(email: string, password: string): Promise<string> {
  const d = await request<Record<string, string>>("/auth/signin", {
    method: "POST",
    body: { email, password },
    auth: false,
  });
  const tok = d.access_token || d.accessToken || d.token;
  if (!tok) throw new ApiError("No access_token in signin response", 500);
  return tok;
}

// ───── charts ─────
export const listCharts = () => request<ChartDef[]>("/admin/stats/charts");
export const chartData = (id: string) =>
  request<QueryResult>(`/admin/stats/charts/${id}/data`);
export const createChart = (c: NewChart) =>
  request<ChartDef>("/admin/stats/charts", { method: "POST", body: c });
export const deleteChart = (id: string) =>
  request<void>(`/admin/stats/charts/${id}`, { method: "DELETE" });

// ───── ad-hoc query ─────
export const runQuery = (sql: string) =>
  request<QueryResult>("/admin/stats/query", { method: "POST", body: { sql } });

// ───── generic admin call (action panel) ─────
export const adminCall = <T = unknown>(
  method: string,
  path: string,
  body?: unknown,
) => request<T>(path, { method, body });
