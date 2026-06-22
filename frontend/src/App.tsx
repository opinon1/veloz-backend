import { useState } from "react";
import { getToken, clearToken } from "./api";
import Login from "./components/Login";
import Dashboard from "./components/Dashboard";
import QueryRunner from "./components/QueryRunner";
import AdminActions from "./components/AdminActions";

type Tab = "dash" | "query" | "actions";

export default function App() {
  const [token, setTokenState] = useState<string>(getToken());
  const [tab, setTab] = useState<Tab>("dash");

  if (!token) {
    return <Login onAuthed={(t) => setTokenState(t)} />;
  }

  const logout = () => {
    clearToken();
    setTokenState("");
  };

  return (
    <div className="app">
      <header>
        <h1>Veloz Ops</h1>
        <nav>
          <button className={tab === "dash" ? "active" : ""} onClick={() => setTab("dash")}>
            Dashboard
          </button>
          <button className={tab === "query" ? "active" : ""} onClick={() => setTab("query")}>
            Query
          </button>
          <button className={tab === "actions" ? "active" : ""} onClick={() => setTab("actions")}>
            Admin Actions
          </button>
        </nav>
        <div className="who">
          <span>admin</span>
          <button className="btn ghost sm" onClick={logout}>
            Logout
          </button>
        </div>
      </header>
      <main>
        {tab === "dash" && <Dashboard />}
        {tab === "query" && <QueryRunner />}
        {tab === "actions" && <AdminActions />}
      </main>
    </div>
  );
}
