import type { ChartDef, NewChart, QueryResult } from "./types";

const BASE = (import.meta.env.VITE_API_BASE as string | undefined) ?? "";
const ACCESS_KEY = "veloz_admin_token";
const REFRESH_KEY = "veloz_refresh_token";

export function getToken(): string {
  return localStorage.getItem(ACCESS_KEY) ?? "";
}
export function setToken(t: string) {
  localStorage.setItem(ACCESS_KEY, t);
}
export function getRefresh(): string {
  return localStorage.getItem(REFRESH_KEY) ?? "";
}
export function setRefresh(t: string) {
  localStorage.setItem(REFRESH_KEY, t);
}
export function clearTokens() {
  localStorage.removeItem(ACCESS_KEY);
  localStorage.removeItem(REFRESH_KEY);
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

/** Low-level fetch → parsed body. Does NOT handle 401/refresh. */
async function raw<T>(path: string, opts: ReqOpts, accessOverride?: string): Promise<{ status: number; data: unknown }> {
  const { method = "GET", body, auth = true } = opts;
  const headers: Record<string, string> = {};
  if (auth) headers["Authorization"] = `Bearer ${accessOverride ?? getToken()}`;
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
  return { status: res.status, data } as { status: number; data: T };
}

function errorOf(status: number, data: unknown): ApiError {
  const msg =
    typeof data === "string"
      ? data
      : (data as { message?: string; error?: string })?.message ||
        (data as { error?: string })?.error ||
        `HTTP ${status}`;
  return new ApiError(msg, status);
}

// ───── refresh (single-flight) ─────
// Access tokens live 15 min. On a 401 we swap the refresh token for a new
// access/refresh pair (the backend ROTATES both and revokes the old refresh,
// so concurrent refreshes would invalidate each other and trip theft
// detection). `refreshing` collapses all concurrent callers onto one request.
let refreshing: Promise<string | null> | null = null;

function refreshAccess(): Promise<string | null> {
  if (refreshing) return refreshing;
  const rt = getRefresh();
  if (!rt) return Promise.resolve(null);

  refreshing = (async () => {
    try {
      const { status, data } = await raw("/auth/refresh", {
        method: "POST",
        body: { refresh_token: rt },
        auth: false,
      });
      if (status < 200 || status >= 300) throw errorOf(status, data);
      const d = data as { access_token: string; refresh_token: string };
      setToken(d.access_token);
      setRefresh(d.refresh_token);
      return d.access_token;
    } catch {
      // Refresh failed/expired → hard logout. Notify the app to show login.
      clearTokens();
      window.dispatchEvent(new Event("veloz:logout"));
      return null;
    } finally {
      refreshing = null;
    }
  })();
  return refreshing;
}

/** Authenticated request with one transparent refresh+retry on 401. */
async function request<T>(path: string, opts: ReqOpts = {}): Promise<T> {
  const first = await raw<T>(path, opts);
  if (first.status === 401 && opts.auth !== false) {
    const newAccess = await refreshAccess();
    if (newAccess) {
      const retry = await raw<T>(path, opts, newAccess);
      if (retry.status >= 200 && retry.status < 300) return retry.data as T;
      throw errorOf(retry.status, retry.data);
    }
  }
  if (first.status < 200 || first.status >= 300) throw errorOf(first.status, first.data);
  return first.data as T;
}

// ───── auth ─────
/** Sign in; stores both tokens and returns the access token. */
export async function signin(email: string, password: string): Promise<string> {
  const { status, data } = await raw("/auth/signin", {
    method: "POST",
    body: { email, password },
    auth: false,
  });
  if (status < 200 || status >= 300) throw errorOf(status, data);
  const d = data as { access_token?: string; refresh_token?: string };
  if (!d.access_token) throw new ApiError("No access_token in signin response", 500);
  setToken(d.access_token);
  if (d.refresh_token) setRefresh(d.refresh_token);
  return d.access_token;
}

// ───── charts ─────
export const listCharts = () => request<ChartDef[]>("/admin/stats/charts");
export const chartData = (id: string) => request<QueryResult>(`/admin/stats/charts/${id}/data`);
export const createChart = (c: NewChart) =>
  request<ChartDef>("/admin/stats/charts", { method: "POST", body: c });
export const deleteChart = (id: string) =>
  request<void>(`/admin/stats/charts/${id}`, { method: "DELETE" });

// ───── ad-hoc query ─────
export const runQuery = (sql: string) =>
  request<QueryResult>("/admin/stats/query", { method: "POST", body: { sql } });

// ───── generic admin call (action panel) ─────
export const adminCall = <T = unknown>(method: string, path: string, body?: unknown) =>
  request<T>(path, { method, body });
