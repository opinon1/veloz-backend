//! Mission cycle + trigger event enums. Mirrors the CHECK constraints
//! on the `missions` table. Wire form: snake_case.

use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// How often a mission's progress resets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionCycle {
    Daily,
    Weekly,
    OneShot,
}

impl MissionCycle {
    pub fn as_str(self) -> &'static str {
        match self {
            MissionCycle::Daily => "daily",
            MissionCycle::Weekly => "weekly",
            MissionCycle::OneShot => "one_shot",
        }
    }

    /// Bucket-key the server derives at write/read time so each cycle's
    /// progress lands on its own row. `one_shot` collapses to a constant
    /// string so the row sticks forever.
    pub fn cycle_key(self, now: DateTime<Utc>) -> String {
        match self {
            MissionCycle::OneShot => "one_shot".to_string(),
            MissionCycle::Daily => now.format("%Y-%m-%d").to_string(),
            MissionCycle::Weekly => {
                let iso = now.iso_week();
                format!("{}-W{:02}", iso.year(), iso.week())
            }
        }
    }
}

impl FromStr for MissionCycle {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "daily" => MissionCycle::Daily,
            "weekly" => MissionCycle::Weekly,
            "one_shot" => MissionCycle::OneShot,
            _ => return Err(()),
        })
    }
}

/// Which app event drives a mission's progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionTriggerEvent {
    RunCompleted,
    CurrencyCollected,
    StorePurchase,
    CharacterLevelUp,
}

impl MissionTriggerEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            MissionTriggerEvent::RunCompleted => "run_completed",
            MissionTriggerEvent::CurrencyCollected => "currency_collected",
            MissionTriggerEvent::StorePurchase => "store_purchase",
            MissionTriggerEvent::CharacterLevelUp => "character_level_up",
        }
    }
}

impl FromStr for MissionTriggerEvent {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "run_completed" => MissionTriggerEvent::RunCompleted,
            "currency_collected" => MissionTriggerEvent::CurrencyCollected,
            "store_purchase" => MissionTriggerEvent::StorePurchase,
            "character_level_up" => MissionTriggerEvent::CharacterLevelUp,
            _ => return Err(()),
        })
    }
}
