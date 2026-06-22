# Veloz Ops Dashboard (frontend)

Standalone React + Vite + TypeScript SPA for the Veloz operational stats
dashboard. Talks to the backend's `/admin/stats/*` JSON API and the existing
`/admin/*` write endpoints. The single-file `entry-point/.../dashboard.html`
served at `/admin/stats` remains as a zero-build fallback; this is the richer
standalone app.

## Develop

```bash
cd frontend
npm install
npm run dev        # http://localhost:5180
```

The dev server proxies `/auth` and `/admin` to `http://localhost:81`, so run the
backend (`docker compose up` at the repo root) alongside it. Sign in with an
admin account, or paste an access token.

## Build

```bash
npm run build      # type-checks, emits static bundle to dist/
npm run preview    # serve the built bundle locally
```

Set `VITE_API_BASE` (see `.env.example`) if the API is on a different origin than
where the bundle is hosted. CORS on the API is already permissive. Deploy `dist/`
to S3 + CloudFront, the app container, or any static host.

## Structure

| Path | Purpose |
|---|---|
| `src/api.ts` | typed fetch wrapper + token handling + endpoint calls |
| `src/types.ts` | `ChartDef`, `QueryResult`, etc. |
| `src/chartSetup.ts` | Chart.js registration + palette |
| `src/components/Login.tsx` | signin / paste-token gate |
| `src/components/Dashboard.tsx` | chart grid + new-chart form |
| `src/components/ChartCard.tsx` | one chart: loads data, refresh/delete |
| `src/components/ChartView.tsx` | renders stat / table / line / bar / pie |
| `src/components/QueryRunner.tsx` | ad-hoc SELECT box + CSV export |
| `src/components/AdminActions.tsx` | grant currency, set role, backfill, raw call |
| `src/components/Toast.tsx` | toast notifications |

## Security

All data and write calls require an admin Bearer token (`AdminClaims` on the
backend). Ad-hoc SQL runs against the backend's read-only `veloz_stats` role and
is wrapped/row-capped server-side — the frontend imposes no additional trust.
