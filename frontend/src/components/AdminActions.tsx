import { useState } from "react";
import { RESOURCES } from "../admin/resources";
import ResourceManager from "./admin/ResourceManager";
import UserManager from "./admin/UserManager";
import BattlepassManager from "./admin/BattlepassManager";
import PrizeWheelManager from "./admin/PrizeWheelManager";
import MiscManager from "./admin/MiscManager";

// Sidebar entries: special managers + every registry resource.
const SPECIAL = [
  { key: "users", label: "Users" },
  { key: "battlepass", label: "Battlepass" },
  { key: "prizewheel", label: "Prize Wheel" },
];
const MISC = { key: "misc", label: "Backfill / Raw" };

export default function AdminActions() {
  const [active, setActive] = useState<string>("users");

  const entries = [...SPECIAL, ...RESOURCES.map((r) => ({ key: r.key, label: r.label })), MISC];

  const renderPane = () => {
    if (active === "users") return <UserManager />;
    if (active === "battlepass") return <BattlepassManager />;
    if (active === "prizewheel") return <PrizeWheelManager />;
    if (active === "misc") return <MiscManager />;
    const def = RESOURCES.find((r) => r.key === active);
    return def ? <ResourceManager def={def} /> : null;
  };

  return (
    <div className="admin-layout">
      <aside className="admin-nav">
        {entries.map((e) => (
          <button
            key={e.key}
            className={active === e.key ? "active" : ""}
            onClick={() => setActive(e.key)}
          >
            {e.label}
          </button>
        ))}
      </aside>
      <div className="admin-pane">{renderPane()}</div>
    </div>
  );
}
