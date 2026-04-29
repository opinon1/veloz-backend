//! Domain enums for the store + wallet currencies.
//!
//! Wire format: snake_case strings (so JSON looks the same as before).
//! Internal use: typed enums so the compiler enforces exhaustive matches —
//! adding a new variant fails the build everywhere it must be handled.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// What kind of thing the store sells.
/// Wire form: lowercase snake_case (`"currency_bundle"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemType {
    Skin,
    Frame,
    CurrencyBundle,
    BpUnlock,
    EnergyRefill,
    Custom,
}

impl ItemType {
    pub fn as_str(self) -> &'static str {
        match self {
            ItemType::Skin => "skin",
            ItemType::Frame => "frame",
            ItemType::CurrencyBundle => "currency_bundle",
            ItemType::BpUnlock => "bp_unlock",
            ItemType::EnergyRefill => "energy_refill",
            ItemType::Custom => "custom",
        }
    }
}

impl FromStr for ItemType {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "skin" => ItemType::Skin,
            "frame" => ItemType::Frame,
            "currency_bundle" => ItemType::CurrencyBundle,
            "bp_unlock" => ItemType::BpUnlock,
            "energy_refill" => ItemType::EnergyRefill,
            "custom" => ItemType::Custom,
            _ => return Err(()),
        })
    }
}

/// Wallet-backed currencies. These map 1:1 to columns on `wallets`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Currency {
    High,
    Soft,
    Energy,
}

impl Currency {
    pub fn as_str(self) -> &'static str {
        match self {
            Currency::High => "high",
            Currency::Soft => "soft",
            Currency::Energy => "energy",
        }
    }
}

impl FromStr for Currency {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "high" => Currency::High,
            "soft" => Currency::Soft,
            "energy" => Currency::Energy,
            _ => return Err(()),
        })
    }
}

/// Currencies a store item can be priced in. Same as `Currency` plus `Iap`,
/// which is a marker meaning "use the IAP receipt flow, not the wallet".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreCurrency {
    High,
    Soft,
    Energy,
    Iap,
}

impl StoreCurrency {
    pub fn as_str(self) -> &'static str {
        match self {
            StoreCurrency::High => "high",
            StoreCurrency::Soft => "soft",
            StoreCurrency::Energy => "energy",
            StoreCurrency::Iap => "iap",
        }
    }

    /// Returns the wallet `Currency` if this is wallet-backed, or None for IAP.
    pub fn as_wallet_currency(self) -> Option<Currency> {
        match self {
            StoreCurrency::High => Some(Currency::High),
            StoreCurrency::Soft => Some(Currency::Soft),
            StoreCurrency::Energy => Some(Currency::Energy),
            StoreCurrency::Iap => None,
        }
    }
}

impl FromStr for StoreCurrency {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "high" => StoreCurrency::High,
            "soft" => StoreCurrency::Soft,
            "energy" => StoreCurrency::Energy,
            "iap" => StoreCurrency::Iap,
            _ => return Err(()),
        })
    }
}

/// One thing the server can grant a player. Elements of:
/// - `store_items.payload` — drives fulfillment on purchase.
/// - `bp_tiers.free_reward` / `premium_reward` — returned to the client on
///   claim (records-only; client + game server apply the grants).
///
/// Wire format is internally tagged:
///   `{"type":"currency","currency":"soft","amount":500}`
///   `{"type":"skin","skin_id":"<uuid>"}`
///
/// Adding a new variant produces compile errors at every `match Grant {}` site.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Grant {
    Currency { currency: Currency, amount: i64 },
    Skin { skin_id: uuid::Uuid },
}

impl Grant {
    /// Cheap shape check (positive amounts, etc.). Doesn't touch DB —
    /// callers still need to verify FK existence (skin still exists/active).
    pub fn validate_shape(&self) -> Result<(), &'static str> {
        match self {
            Grant::Currency { amount, .. } => {
                if *amount <= 0 {
                    return Err("currency amount must be > 0");
                }
            }
            Grant::Skin { .. } => {}
        }
        Ok(())
    }
}

/// Parse + validate a `Vec<Grant>` payload from raw JSON. Empty arrays are
/// rejected — every store item / BP tier reward must grant something
/// concrete or the whole "you bought a thing" flow becomes a no-op.
pub fn validate_grants(json: &serde_json::Value) -> Result<Vec<Grant>, &'static str> {
    let arr = json.as_array().ok_or("grants must be a JSON array")?;
    if arr.is_empty() {
        return Err("grants array must not be empty");
    }
    let mut out = Vec::with_capacity(arr.len());
    for v in arr {
        let g: Grant = serde_json::from_value(v.clone())
            .map_err(|_| "invalid grant element")?;
        g.validate_shape()?;
        out.push(g);
    }
    Ok(out)
}
