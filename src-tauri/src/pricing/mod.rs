use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
mod estimate;
mod registry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceError {
    InvalidModel,
    InvalidPrice,
    InvalidRegistry,
    Incompatible,
    InvalidTokens,
    ArithmeticOverflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CacheWriteTtl {
    FiveMinutes,
    OneHour,
    ThirtyMinutes,
    TwelveHours,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InputContext {
    Short,
    Long,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenClass {
    FreshInput,
    CacheRead,
    CacheWrite,
    Output,
    Reasoning,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PriceUnavailable {
    UnknownModel,
    MissingTokens,
    MissingContext,
    MissingCacheTtl,
    MissingPrice,
    MissingAccounting,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CacheWritePrice {
    pub ttl: CacheWriteTtl,
    pub usd_per_million: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PriceRates {
    pub fresh_input_usd_per_million: Option<f64>,
    pub cache_read_usd_per_million: Option<f64>,
    pub cache_write: Vec<CacheWritePrice>,
    pub output_usd_per_million: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelPrice {
    pub model_id: String,
    pub revision: String,
    pub retrieved_on: String,
    pub source_urls: Vec<String>,
    pub long_context_threshold_tokens: Option<u64>,
    pub short_context: PriceRates,
    pub long_context: PriceRates,
}
pub type ManualPriceOverride = ModelPrice;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BillingTokens {
    pub fresh_input: Option<u64>,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
    pub output: Option<u64>,
    pub reasoning: Option<u64>,
    pub output_includes_reasoning: Option<bool>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PricingContext {
    pub input_context: Option<InputContext>,
    pub cache_write_ttl: Option<CacheWriteTtl>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostComponent {
    pub class: TokenClass,
    pub tokens: Option<u64>,
    pub usd_per_million: Option<f64>,
    pub usd: Option<f64>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoneyEstimate {
    pub model_id: String,
    pub label: String,
    pub currency: String,
    pub basis: String,
    pub price_revision: Option<String>,
    pub retrieved_on: Option<String>,
    pub source_urls: Vec<String>,
    pub manual_override: bool,
    pub context: PricingContext,
    pub reasoning_included_in_output: Option<bool>,
    pub usd: Option<f64>,
    pub components: Vec<CostComponent>,
    pub unavailable: Option<PriceUnavailable>,
}

pub struct PriceRegistry {
    revision: String,
    models: BTreeMap<String, ModelPrice>,
    overrides: BTreeMap<String, ManualPriceOverride>,
}
