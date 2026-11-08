use super::AppState;
use crate::{
    model::DiagnosticCode,
    pricing::{
        BillingTokens, ManualPriceOverride, ModelPrice, MoneyEstimate, PriceRegistry,
        PricingContext,
    },
    storage::PreferenceKey,
};
use serde::Serialize;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingView {
    pub prices: Vec<ModelPrice>,
    pub overrides: Vec<ManualPriceOverride>,
}
impl AppState {
    fn price_registry(&self) -> Result<PriceRegistry, DiagnosticCode> {
        let mut registry = PriceRegistry::bundled().map_err(|_| DiagnosticCode::StorageCorrupt)?;
        let overrides: Vec<ManualPriceOverride> = self
            .load_preference(PreferenceKey::Prices)?
            .unwrap_or_default();
        for price in overrides {
            registry
                .set_override(price)
                .map_err(|_| DiagnosticCode::StorageCorrupt)?;
        }
        Ok(registry)
    }
    pub fn pricing_view(&self) -> Result<PricingView, DiagnosticCode> {
        let registry = self.price_registry()?;
        Ok(PricingView {
            prices: registry.prices(),
            overrides: registry.overrides(),
        })
    }
    pub fn set_price_override(
        &self,
        price: ManualPriceOverride,
    ) -> Result<PricingView, DiagnosticCode> {
        let mut registry = self.price_registry()?;
        registry
            .set_override(price)
            .map_err(|_| DiagnosticCode::InvalidHistoryRequest)?;
        self.save_preference(PreferenceKey::Prices, &registry.overrides())?;
        self.pricing_view()
    }
    pub fn remove_price_override(&self, model_id: &str) -> Result<PricingView, DiagnosticCode> {
        let mut registry = self.price_registry()?;
        registry
            .remove_override(model_id)
            .map_err(|_| DiagnosticCode::InvalidHistoryRequest)?;
        self.save_preference(PreferenceKey::Prices, &registry.overrides())?;
        self.pricing_view()
    }
    pub fn estimate_cost(
        &self,
        model_id: &str,
        tokens: &BillingTokens,
        context: &PricingContext,
    ) -> Result<MoneyEstimate, DiagnosticCode> {
        self.price_registry()?
            .estimate(model_id, tokens, context)
            .map_err(|_| DiagnosticCode::InvalidHistoryRequest)
    }
}
