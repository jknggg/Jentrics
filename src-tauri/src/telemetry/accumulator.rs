use super::{parser::Point, IngestResult, TelemetryDelta, TelemetryError, MAX_SERIES};
use std::collections::HashMap;

pub(super) fn apply(
    series: &mut HashMap<u64, Point>,
    point: Point,
    result: &mut IngestResult,
) -> Result<(), TelemetryError> {
    let mut value = point.value;
    let mut count = point.count;
    let mut start = point.start;
    let mut gap = false;
    if let Some(previous) = series.get(&point.key) {
        if point.cumulative != previous.cumulative {
            return Err(TelemetryError::Conflict);
        }
        if point.start == previous.start && point.end == previous.end {
            if point.value != previous.value || point.count != previous.count {
                return Err(TelemetryError::Conflict);
            }
            result.duplicate_points += 1;
            return Ok(());
        }
        if point.end <= previous.end {
            return Err(TelemetryError::OutOfOrder);
        }
        if point.cumulative {
            if point.start != previous.start {
                if point.start < previous.end {
                    return Err(TelemetryError::Conflict);
                }
                result.baseline_points += 1;
                series.insert(point.key, point);
                return Ok(());
            }
            if point.value < previous.value {
                return Err(TelemetryError::Conflict);
            }
            value -= previous.value;
            count = match (point.count, previous.count) {
                (Some(current), Some(previous)) => Some(
                    current
                        .checked_sub(previous)
                        .ok_or(TelemetryError::Conflict)?,
                ),
                (None, None) => None,
                _ => return Err(TelemetryError::Conflict),
            };
            start = previous.end;
        } else {
            if point.start < previous.end {
                return Err(TelemetryError::Conflict);
            }
            gap = point.start > previous.end;
        }
    } else {
        if series.len() >= MAX_SERIES {
            return Err(TelemetryError::Capacity);
        }
        if point.cumulative {
            result.baseline_points += 1;
            series.insert(point.key, point);
            return Ok(());
        }
    }
    let delta = TelemetryDelta {
        series_key: format!("defaultHasherV1:{:016x}", point.key),
        provider: point.provider,
        metric: point.metric,
        dimension: point.dimension.clone(),
        model: point.model.clone(),
        start_ms: (start / 1_000_000) as i64,
        end_ms: (point.end / 1_000_000) as i64,
        start_ns: start.to_string(),
        end_ns: point.end.to_string(),
        counter_start_ns: point.start.to_string(),
        value,
        count,
        gap_before: gap,
    };
    super::validate_delta(&delta)?;
    result.deltas.push(delta);
    series.insert(point.key, point);
    Ok(())
}
