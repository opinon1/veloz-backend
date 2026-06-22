import { useState } from "react";
import { signin, setToken, ApiError } from "../api";
import { useToast } from "./Toast";

export default function Login({ onAuthed }: { onAuthed: (token: string) => void }) {
  const toast = useToast();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [paste, setPaste] = useState("");
  const [busy, setBusy] = useState(false);

  const accept = (t: string) => {
    setToken(t);
    onAuthed(t);
  };

  const doSignin = async () => {
    setBusy(true);
    try {
      accept(await signin(email, password));
    } catch (e) {
      toast(e instanceof ApiError ? e.message : "Sign in failed", "err");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="login panel">
      <h2>Veloz Ops Dashboard</h2>
      <p className="hint">Sign in with an admin account, or paste an access token.</p>

      <label>Email</label>
      <input
        type="email"
        autoComplete="username"
        value={email}
        onChange={(e) => setEmail(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && doSignin()}
      />
      <label>Password</label>
      <input
        type="password"
        autoComplete="current-password"
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && doSignin()}
      />
      <div style={{ marginTop: 14 }}>
        <button className="btn" disabled={busy} onClick={doSignin}>
          {busy ? "Signing in…" : "Sign in"}
        </button>
      </div>

      <label style={{ marginTop: 18 }}>…or paste access token</label>
      <input value={paste} placeholder="Bearer token" onChange={(e) => setPaste(e.target.value)} />
      <div style={{ marginTop: 10 }}>
        <button className="btn ghost" disabled={!paste.trim()} onClick={() => accept(paste.trim())}>
          Use token
        </button>
      </div>
    </div>
  );
}
