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
