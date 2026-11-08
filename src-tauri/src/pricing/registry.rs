use super::{ManualPriceOverride, ModelPrice, PriceError, PriceRates, PriceRegistry};
use serde::Deserialize;
use std::collections::BTreeMap;

pub(super) const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistryDocument {
    schema_version: u8,
    revision: String,
    models: Vec<ModelPrice>,
}

pub(super) fn validate_model_id(model_id: &str) -> Result<(), PriceError> {
    if model_id.is_empty()
        || model_id.len() > 128
        || !model_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_./:-".contains(&byte))
    {
        return Err(PriceError::InvalidModel);
    }
    Ok(())
}

fn valid_revision(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.-".contains(&byte))
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return false;
    }
    let Ok(year) = value[..4].parse::<u16>() else {
        return false;
    };
    let Ok(month) = value[5..7].parse::<u8>() else {
        return false;
    };
    let Ok(day) = value[8..].parse::<u8>() else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    year > 0 && (1..=days).contains(&day)
}

fn validate_rates(rates: &PriceRates) -> Result<(), PriceError> {
    let mut numbers = vec![
        rates.fresh_input_usd_per_million,
        rates.cache_read_usd_per_million,
        rates.output_usd_per_million,
    ];
    for (index, write) in rates.cache_write.iter().enumerate() {
        if rates.cache_write[..index]
            .iter()
            .any(|other| other.ttl == write.ttl)
        {
            return Err(PriceError::InvalidPrice);
        }
        numbers.push(write.usd_per_million);
    }
    if numbers
        .into_iter()
        .flatten()
        .any(|number| !number.is_finite() || number < 0.0)
    {
        return Err(PriceError::InvalidPrice);
    }
    Ok(())
}

fn validate_model(price: &ModelPrice) -> Result<(), PriceError> {
    validate_model_id(&price.model_id)?;
    if !valid_revision(&price.revision)
        || !valid_date(&price.retrieved_on)
        || price.source_urls.iter().any(|url| {
            !url.starts_with("https://") || url.len() > 512 || url.chars().any(char::is_whitespace)
        })
        || price
            .long_context_threshold_tokens
            .is_some_and(|value| value == 0 || value > MAX_SAFE_INTEGER)
        || (price.long_context_threshold_tokens.is_none()
            && price.short_context != price.long_context)
    {
        return Err(PriceError::InvalidPrice);
    }
    validate_rates(&price.short_context)?;
    validate_rates(&price.long_context)
}

impl PriceRegistry {
    pub fn prices(&self) -> Vec<ModelPrice> {
        let mut values = self.models.clone();
        values.extend(self.overrides.clone());
        values.into_values().collect()
    }
    pub fn bundled() -> Result<Self, PriceError> {
        Self::from_json(include_str!("registry.json"))
    }

    pub fn from_json(encoded: &str) -> Result<Self, PriceError> {
        let value: serde_json::Value =
            serde_json::from_str(encoded).map_err(|_| PriceError::InvalidRegistry)?;
        let version = value
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
            .ok_or(PriceError::InvalidRegistry)?;
        if version > 1 {
            return Err(PriceError::Incompatible);
        }
        let document: RegistryDocument =
            serde_json::from_value(value).map_err(|_| PriceError::InvalidRegistry)?;
        if document.schema_version != 1
            || !valid_revision(&document.revision)
            || document.models.is_empty()
        {
            return Err(PriceError::InvalidRegistry);
        }
        let mut models = BTreeMap::new();
        for model in document.models {
            validate_model(&model).map_err(|_| PriceError::InvalidRegistry)?;
            if model.source_urls.is_empty()
                || models.insert(model.model_id.clone(), model).is_some()
            {
                return Err(PriceError::InvalidRegistry);
            }
        }
        Ok(Self {
            revision: document.revision,
            models,
            overrides: BTreeMap::new(),
        })
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }

    pub fn model_price(&self, model_id: &str) -> Option<&ModelPrice> {
        self.overrides
            .get(model_id)
            .or_else(|| self.models.get(model_id))
    }

    pub fn set_override(&mut self, price: ManualPriceOverride) -> Result<(), PriceError> {
        validate_model(&price)?;
        self.overrides.insert(price.model_id.clone(), price);
        Ok(())
    }

    pub fn remove_override(&mut self, model_id: &str) -> Result<(), PriceError> {
        validate_model_id(model_id)?;
        self.overrides.remove(model_id);
        Ok(())
    }

    pub fn overrides(&self) -> Vec<ManualPriceOverride> {
        self.overrides.values().cloned().collect()
    }
}
