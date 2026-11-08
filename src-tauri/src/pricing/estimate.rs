use super::{
    registry::{validate_model_id, MAX_SAFE_INTEGER},
    BillingTokens, CostComponent, InputContext, MoneyEstimate, PriceError, PriceRegistry,
    PriceUnavailable, PricingContext, TokenClass,
};

fn component(
    class: TokenClass,
    tokens: Option<u64>,
    rate: Option<f64>,
) -> Result<CostComponent, PriceError> {
    let usd = match (tokens, rate) {
        (Some(0), _) => Some(0.0),
        (Some(tokens), Some(rate)) => {
            let amount = rate * (tokens as f64 / 1_000_000.0);
            if !amount.is_finite() {
                return Err(PriceError::ArithmeticOverflow);
            }
            Some(amount)
        }
        _ => None,
    };
    Ok(CostComponent {
        class,
        tokens,
        usd_per_million: rate,
        usd,
    })
}

impl PriceRegistry {
    /// Estimates standard global API token prices, excluding tools, taxes,
    /// negotiated discounts and subscription billing. Inputs must be disjoint.
    pub fn estimate(
        &self,
        model_id: &str,
        tokens: &BillingTokens,
        context: &PricingContext,
    ) -> Result<MoneyEstimate, PriceError> {
        validate_model_id(model_id)?;
        if [
            tokens.fresh_input,
            tokens.cache_read,
            tokens.cache_write,
            tokens.output,
            tokens.reasoning,
        ]
        .into_iter()
        .flatten()
        .any(|value| value > MAX_SAFE_INTEGER)
            || (tokens.output_includes_reasoning == Some(true)
                && matches!((tokens.output,tokens.reasoning),(Some(output),Some(reasoning)) if reasoning>output))
        {
            return Err(PriceError::InvalidTokens);
        }
        let mut estimate = MoneyEstimate {
            model_id: model_id.to_owned(),
            label: "apiEquivalentEstimate".into(),
            currency: "USD".into(),
            basis: "standardApiReference".into(),
            price_revision: None,
            retrieved_on: None,
            source_urls: vec![],
            manual_override: self.overrides.contains_key(model_id),
            context: context.clone(),
            reasoning_included_in_output: tokens.output_includes_reasoning,
            usd: None,
            components: vec![],
            unavailable: None,
        };
        let Some(price) = self.model_price(model_id) else {
            estimate.unavailable = Some(PriceUnavailable::UnknownModel);
            return Ok(estimate);
        };
        estimate.price_revision = Some(price.revision.clone());
        estimate.retrieved_on = Some(price.retrieved_on.clone());
        estimate.source_urls = price.source_urls.clone();
        let rates = match (price.long_context_threshold_tokens, context.input_context) {
            (Some(_), None) => {
                estimate.unavailable = Some(PriceUnavailable::MissingContext);
                return Ok(estimate);
            }
            (_, Some(InputContext::Long)) => &price.long_context,
            // Flat-context entries have equal rate sets, validated on loading.
            (None, None) | (_, Some(InputContext::Short)) => &price.short_context,
        };
        let Some(includes_reasoning) = tokens.output_includes_reasoning else {
            estimate.unavailable = Some(PriceUnavailable::MissingAccounting);
            return Ok(estimate);
        };
        if tokens.cache_write.is_some_and(|value| value > 0) && context.cache_write_ttl.is_none() {
            estimate.unavailable = Some(PriceUnavailable::MissingCacheTtl);
            return Ok(estimate);
        }
        let write_rate = context
            .cache_write_ttl
            .and_then(|ttl| rates.cache_write.iter().find(|rate| rate.ttl == ttl))
            .and_then(|rate| rate.usd_per_million);
        estimate.components = vec![
            component(
                TokenClass::FreshInput,
                tokens.fresh_input,
                rates.fresh_input_usd_per_million,
            )?,
            component(
                TokenClass::CacheRead,
                tokens.cache_read,
                rates.cache_read_usd_per_million,
            )?,
            component(TokenClass::CacheWrite, tokens.cache_write, write_rate)?,
            component(
                TokenClass::Output,
                tokens.output,
                rates.output_usd_per_million,
            )?,
        ];
        if !includes_reasoning {
            estimate.components.push(component(
                TokenClass::Reasoning,
                tokens.reasoning,
                rates.output_usd_per_million,
            )?);
        }
        if estimate.components.iter().any(|part| part.tokens.is_none()) {
            estimate.unavailable = Some(PriceUnavailable::MissingTokens);
        } else if estimate.components.iter().any(|part| part.usd.is_none()) {
            estimate.unavailable = Some(PriceUnavailable::MissingPrice);
        } else {
            let mut total = 0.0;
            for part in &estimate.components {
                total += part.usd.ok_or(PriceError::ArithmeticOverflow)?;
            }
            if !total.is_finite() {
                return Err(PriceError::ArithmeticOverflow);
            }
            estimate.usd = Some(total);
        }
        Ok(estimate)
    }
}
