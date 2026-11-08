use jentrics_core::pricing::*;

fn tokens() -> BillingTokens {
    BillingTokens {
        fresh_input: Some(1000),
        cache_read: Some(1000),
        cache_write: Some(1000),
        output: Some(1000),
        reasoning: Some(200),
        output_includes_reasoning: Some(true),
    }
}
fn context() -> PricingContext {
    PricingContext {
        input_context: Some(InputContext::Short),
        cache_write_ttl: Some(CacheWriteTtl::ThirtyMinutes),
    }
}
fn approximately(actual: Option<f64>, expected: f64) {
    assert!((actual.unwrap() - expected).abs() < 1e-12);
}

#[test]
fn standard_reference_cost_prices_disjoint_classes_without_double_reasoning() {
    let registry = PriceRegistry::bundled().unwrap();
    let estimate = registry
        .estimate("gpt-6.1-sol", &tokens(), &context())
        .unwrap();
    approximately(estimate.usd, 0.0146);
    assert_eq!(estimate.label, "apiEquivalentEstimate");
    assert_eq!(estimate.currency, "USD");
    assert_eq!(estimate.basis, "standardApiReference");
    assert_eq!(estimate.price_revision.as_deref(), Some("2026-10-30.1"));
    assert_eq!(estimate.retrieved_on.as_deref(), Some("2026-10-30"));
    assert!(!estimate.manual_override);
    assert!(estimate
        .source_urls
        .iter()
        .any(|url| url == "https://developers.openai.com/api/docs/models/gpt-6.1-sol"));
    assert_eq!(estimate.unavailable, None);
    assert_eq!(estimate.components.len(), 4);
    assert!(!estimate
        .components
        .iter()
        .any(|part| part.class == TokenClass::Reasoning));
}

#[test]
fn reasoning_excluded_from_output_is_added_exactly_once() {
    let registry = PriceRegistry::bundled().unwrap();
    let mut value = tokens();
    value.output_includes_reasoning = Some(false);
    approximately(
        registry
            .estimate("gpt-6.1-sol", &value, &context())
            .unwrap()
            .usd,
        0.0166,
    );
    value.reasoning = None;
    let estimate = registry
        .estimate("gpt-6.1-sol", &value, &context())
        .unwrap();
    assert_eq!(estimate.usd, None);
    assert_eq!(estimate.unavailable, Some(PriceUnavailable::MissingTokens));
    value.output_includes_reasoning = Some(true);
    approximately(
        registry
            .estimate("gpt-6.1-sol", &value, &context())
            .unwrap()
            .usd,
        0.0146,
    );
    value.output_includes_reasoning = None;
    assert_eq!(
        registry
            .estimate("gpt-6.1-sol", &value, &context())
            .unwrap()
            .unavailable,
        Some(PriceUnavailable::MissingAccounting)
    );
}

#[test]
fn long_context_and_cache_lifetime_use_their_own_rates() {
    let registry = PriceRegistry::bundled().unwrap();
    let mut value = context();
    value.input_context = Some(InputContext::Long);
    approximately(
        registry
            .estimate("gpt-6.1-sol", &tokens(), &value)
            .unwrap()
            .usd,
        0.0242,
    );
    value.cache_write_ttl = Some(CacheWriteTtl::TwelveHours);
    approximately(
        registry
            .estimate("gpt-6.1-sol", &tokens(), &value)
            .unwrap()
            .usd,
        0.0292,
    );
    approximately(
        registry
            .estimate("gpt-6.1-terra", &tokens(), &value)
            .unwrap()
            .usd,
        0.0146,
    );
    assert_eq!(
        registry
            .model_price("gpt-6.1-sol")
            .unwrap()
            .long_context_threshold_tokens,
        Some(272_000)
    );
}

#[test]
fn exact_claude_ids_have_documented_global_rates_without_cli_alias_guessing() {
    let registry = PriceRegistry::bundled().unwrap();
    let mut value = context();
    value.cache_write_ttl = Some(CacheWriteTtl::FiveMinutes);
    approximately(
        registry
            .estimate("claude-opus-5-5", &tokens(), &value)
            .unwrap()
            .usd,
        0.0292,
    );
    value.cache_write_ttl = Some(CacheWriteTtl::OneHour);
    approximately(
        registry
            .estimate("claude-sonnet-5-5", &tokens(), &value)
            .unwrap()
            .usd,
        0.0161,
    );
    value.input_context = Some(InputContext::Long);
    approximately(
        registry
            .estimate("claude-sonnet-5-5", &tokens(), &value)
            .unwrap()
            .usd,
        0.0161,
    );
    for unknown in [
        "opus",
        "sonnet",
        "gpt-6.1-sol-latest",
        "provider/claude-sonnet-5-5",
        "unknown",
    ] {
        let estimate = registry.estimate(unknown, &tokens(), &value).unwrap();
        assert_eq!(estimate.usd, None);
        assert_eq!(estimate.unavailable, Some(PriceUnavailable::UnknownModel));
    }
}

#[test]
fn missing_inputs_price_or_context_are_unavailable_and_zero_remains_known() {
    let registry = PriceRegistry::bundled().unwrap();
    let mut value = context();
    value.input_context = None;
    assert_eq!(
        registry
            .estimate("gpt-6.1-sol", &tokens(), &value)
            .unwrap()
            .unavailable,
        Some(PriceUnavailable::MissingContext)
    );
    value.input_context = Some(InputContext::Short);
    value.cache_write_ttl = None;
    assert_eq!(
        registry
            .estimate("gpt-6.1-sol", &tokens(), &value)
            .unwrap()
            .unavailable,
        Some(PriceUnavailable::MissingCacheTtl)
    );
    value.cache_write_ttl = Some(CacheWriteTtl::FiveMinutes);
    assert_eq!(
        registry
            .estimate("gpt-6.1-sol", &tokens(), &value)
            .unwrap()
            .unavailable,
        Some(PriceUnavailable::MissingPrice)
    );
    let mut counts = tokens();
    counts.cache_write = Some(0);
    counts.cache_read = Some(0);
    counts.fresh_input = Some(0);
    counts.output = Some(0);
    counts.reasoning = Some(0);
    approximately(
        registry
            .estimate("gpt-6.1-sol", &counts, &value)
            .unwrap()
            .usd,
        0.0,
    );
    counts.fresh_input = None;
    assert_eq!(
        registry
            .estimate("gpt-6.1-sol", &counts, &value)
            .unwrap()
            .usd,
        None
    );
}

#[test]
fn manual_override_is_exact_versioned_removable_and_preserves_unknown_rates() {
    let mut registry = PriceRegistry::bundled().unwrap();
    let mut manual = registry.model_price("gpt-6.1-sol").unwrap().clone();
    manual.revision = "manual-2026-10-30".into();
    manual.short_context.fresh_input_usd_per_million = Some(4.0);
    registry.set_override(manual.clone()).unwrap();
    let estimate = registry
        .estimate("gpt-6.1-sol", &tokens(), &context())
        .unwrap();
    approximately(estimate.usd, 0.0166);
    assert!(estimate.manual_override);
    assert_eq!(
        estimate.price_revision.as_deref(),
        Some("manual-2026-10-30")
    );
    assert_eq!(registry.overrides(), vec![manual.clone()]);
    manual.short_context.output_usd_per_million = None;
    registry.set_override(manual).unwrap();
    assert_eq!(
        registry
            .estimate("gpt-6.1-sol", &tokens(), &context())
            .unwrap()
            .unavailable,
        Some(PriceUnavailable::MissingPrice)
    );
    registry.remove_override("gpt-6.1-sol").unwrap();
    approximately(
        registry
            .estimate("gpt-6.1-sol", &tokens(), &context())
            .unwrap()
            .usd,
        0.0146,
    );
    assert!(registry.overrides().is_empty());
}

#[test]
fn invalid_prices_tokens_and_registry_versions_fail_explicitly() {
    let mut registry = PriceRegistry::bundled().unwrap();
    let original = registry.model_price("gpt-6.1-sol").unwrap().clone();
    for number in [f64::NAN, f64::INFINITY, -1.0] {
        let mut manual = original.clone();
        manual.short_context.fresh_input_usd_per_million = Some(number);
        assert_eq!(registry.set_override(manual), Err(PriceError::InvalidPrice));
        assert!(registry.overrides().is_empty());
    }
    let mut invalid = tokens();
    invalid.reasoning = Some(1001);
    assert_eq!(
        registry.estimate("gpt-6.1-sol", &invalid, &context()),
        Err(PriceError::InvalidTokens)
    );
    invalid = tokens();
    invalid.fresh_input = Some(u64::MAX);
    assert_eq!(
        registry.estimate("gpt-6.1-sol", &invalid, &context()),
        Err(PriceError::InvalidTokens)
    );
    assert!(matches!(
        PriceRegistry::from_json(r#"{"schemaVersion":99,"revision":"future","models":[]}"#),
        Err(PriceError::Incompatible)
    ));
    assert!(matches!(
        PriceRegistry::from_json(r#"{"schemaVersion":1,"revision":"v1","models":[]}"#),
        Err(PriceError::InvalidRegistry)
    ));
    assert!(matches!(
        PriceRegistry::from_json("not json"),
        Err(PriceError::InvalidRegistry)
    ));
}

#[test]
fn manual_custom_model_roundtrips_without_borrowing_another_models_price() {
    let mut registry = PriceRegistry::bundled().unwrap();
    let mut manual = registry.model_price("gpt-6.1-sol").unwrap().clone();
    manual.model_id = "selfhost/custom-exact".into();
    manual.revision = "user-2026-10-30.2".into();
    manual.source_urls.clear();
    let encoded = serde_json::to_string(&manual).unwrap();
    let restored: ManualPriceOverride = serde_json::from_str(&encoded).unwrap();
    registry.set_override(restored).unwrap();
    assert!(
        registry
            .estimate("selfhost/custom-exact", &tokens(), &context())
            .unwrap()
            .manual_override
    );
    approximately(
        registry
            .estimate("selfhost/custom-exact", &tokens(), &context())
            .unwrap()
            .usd,
        0.0146,
    );
    assert_eq!(
        registry
            .estimate("selfhost/custom", &tokens(), &context())
            .unwrap()
            .unavailable,
        Some(PriceUnavailable::UnknownModel)
    );
    registry.remove_override("selfhost/custom-exact").unwrap();
    assert_eq!(
        registry
            .estimate("selfhost/custom-exact", &tokens(), &context())
            .unwrap()
            .unavailable,
        Some(PriceUnavailable::UnknownModel)
    );
}

#[test]
fn malformed_override_metadata_and_ambiguous_cache_prices_fail_without_replacing_good_prices() {
    let mut registry = PriceRegistry::bundled().unwrap();
    let original = registry.model_price("gpt-6.1-sol").unwrap().clone();
    for mutation in 0..7 {
        let mut manual = original.clone();
        match mutation {
            0 => manual.model_id = "private\npath".into(),
            1 => manual.retrieved_on = "2026-02-29".into(),
            2 => manual.revision.clear(),
            3 => manual
                .short_context
                .cache_write
                .push(manual.short_context.cache_write[0].clone()),
            4 => manual.long_context_threshold_tokens = Some(0),
            5 => manual.long_context_threshold_tokens = None,
            _ => manual.source_urls = vec!["not-a-source".into()],
        }
        assert!(registry.set_override(manual).is_err());
        assert_eq!(registry.model_price("gpt-6.1-sol"), Some(&original));
        assert!(registry.overrides().is_empty());
    }
    let mut huge = original;
    huge.short_context.output_usd_per_million = Some(f64::MAX);
    registry.set_override(huge).unwrap();
    let mut counts = tokens();
    counts.output = Some(9_007_199_254_740_991);
    assert_eq!(
        registry.estimate("gpt-6.1-sol", &counts, &context()),
        Err(PriceError::ArithmeticOverflow)
    );
}
