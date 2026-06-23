// Registry of standard CRUD admin resources (list / create / edit / delete).
// Templates mirror the Bruno collection bodies exactly. Special resources with
// nesting or singletons (Users, Battlepass, Prize Wheel, Misc) have dedicated
// managers instead of living here.

export interface ResourceDef {
  key: string;
  label: string;
  /** Base path, e.g. "/admin/store". List=GET base, create=POST base,
   *  edit=PATCH base/{id}, delete=DELETE base/{id}. */
  path: string;
  /** Columns to show in the list table (besides id). */
  cols: string[];
  /** Body template for "New". */
  template: Record<string, unknown>;
  noCreate?: boolean;
  noDelete?: boolean;
}

export const RESOURCES: ResourceDef[] = [
  {
    key: "store",
    label: "Store items",
    path: "/admin/store",
    cols: ["name", "item_type", "cost", "currency", "is_active", "is_default"],
    template: {
      name: "Starter Pack",
      description: "1 skin + 500 soft + 5 energy",
      item_type: "custom",
      cost: 10,
      currency: "high",
      iap_product_id: null,
      payload: [
        { type: "currency", currency: "soft", amount: 500 },
        { type: "currency", currency: "energy", amount: 5 },
      ],
      metadata: { badge: "popular" },
      is_default: false,
    },
  },
  {
    key: "skins",
    label: "Skins",
    path: "/admin/skins",
    cols: ["character_id", "cost", "currency", "is_default", "is_active"],
    template: {
      character_id: "",
      cost: 500,
      currency: "soft",
      is_default: false,
      metadata: {},
    },
  },
  {
    key: "characters",
    label: "Characters",
    path: "/admin/characters",
    cols: ["name", "rarity", "default_unlocked", "is_active"],
    template: {
      name: "Runner",
      default_unlocked: false,
      rarity: "common",
      metadata: { sort_order: 1 },
    },
  },
  {
    key: "avatars",
    label: "Avatars",
    path: "/admin/avatars",
    cols: ["name", "price", "currency", "is_default", "is_active"],
    template: {
      name: "Galaxy Avatar",
      price: 250,
      currency: "soft",
      is_default: false,
    },
  },
  {
    key: "frames",
    label: "Frames",
    path: "/admin/frames",
    cols: ["name", "price", "currency", "is_default", "is_active"],
    template: {
      name: "Gold Frame",
      price: 500,
      currency: "high",
      is_default: false,
    },
  },
  {
    key: "missions",
    label: "Missions",
    path: "/admin/missions",
    cols: ["name", "cycle", "trigger_event", "xp_reward", "is_active"],
    template: {
      name: "Run 10 times daily",
      description: "Play 10 matches today",
      cycle: "daily",
      trigger_event: "run_completed",
      target: { amount: 10 },
      xp_reward: 500,
      is_active: true,
    },
  },
];

/** Fields stripped from a row before pre-filling the edit JSON editor. */
export const READONLY_FIELDS = ["id", "created_at", "updated_at"];
