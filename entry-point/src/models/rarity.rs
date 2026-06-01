//! Character rarity. Multiplier feeds the cards-required-to-level-up
//! formula (cards system itself still TODO). Wire form: snake_case.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

impl Rarity {
    pub fn as_str(self) -> &'static str {
        match self {
            Rarity::Common => "common",
            Rarity::Uncommon => "uncommon",
            Rarity::Rare => "rare",
            Rarity::Epic => "epic",
            Rarity::Legendary => "legendary",
        }
    }

    /// Per-spec multiplier used in cards-required-to-level-up math.
    pub fn multiplier(self) -> f64 {
        match self {
            Rarity::Common => 1.0,
            Rarity::Uncommon => 1.25,
            Rarity::Rare => 1.5,
            Rarity::Epic => 1.75,
            Rarity::Legendary => 2.0,
        }
    }
}

impl FromStr for Rarity {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "common" => Rarity::Common,
            "uncommon" => Rarity::Uncommon,
            "rare" => Rarity::Rare,
            "epic" => Rarity::Epic,
            "legendary" => Rarity::Legendary,
            _ => return Err(()),
        })
    }
}
