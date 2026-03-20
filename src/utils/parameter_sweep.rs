use anyhow::{anyhow, Result};
use std::collections::HashMap;

use super::parsers::parse_range_spec;

/// Parse a single parameter spec like "name=val1,val2,val3".
pub fn parse_param_spec(spec: &str) -> Result<(String, Vec<String>)> {
    let parts: Vec<&str> = spec.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err(anyhow!(
            "Invalid param format. Expected 'name=val1,val2,...'"
        ));
    }

    let name = parts[0].trim().to_string();
    if name.is_empty() {
        return Err(anyhow!("Parameter name cannot be empty"));
    }

    let value_spec = parts[1];
    let colon_count = value_spec.matches(':').count();
    let values = if (1..=2).contains(&colon_count) && !value_spec.contains(',') {
        parse_range_spec(value_spec)?
    } else {
        value_spec
            .split(',')
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .collect()
    };

    if values.is_empty() {
        return Err(anyhow!("Parameter must have at least one value"));
    }

    Ok((name, values))
}

/// Generate cartesian product of parameter values.
pub fn generate_param_combinations(
    param_specs: &[(String, Vec<String>)],
) -> Vec<HashMap<String, String>> {
    if param_specs.is_empty() {
        return vec![HashMap::new()];
    }

    let mut combinations = vec![HashMap::new()];

    for (param_name, values) in param_specs {
        let mut new_combinations = Vec::with_capacity(combinations.len() * values.len());
        for combo in &combinations {
            for value in values {
                let mut new_combo = combo.clone();
                new_combo.insert(param_name.to_string(), value.to_string());
                new_combinations.push(new_combo);
            }
        }
        combinations = new_combinations;
    }

    combinations
}

#[cfg(test)]
mod tests {
    use super::{generate_param_combinations, parse_param_spec};

    #[test]
    fn parse_param_spec_supports_csv_values() {
        let (name, values) = parse_param_spec("lr=0.001,0.01").unwrap();

        assert_eq!(name, "lr");
        assert_eq!(values, vec!["0.001", "0.01"]);
    }

    #[test]
    fn parse_param_spec_supports_ranges() {
        let (name, values) = parse_param_spec("seed=1:3").unwrap();

        assert_eq!(name, "seed");
        assert_eq!(values, vec!["1", "2", "3"]);
    }

    #[test]
    fn generate_param_combinations_builds_cartesian_product() {
        let combos = generate_param_combinations(&[
            ("lr".into(), vec!["1".into(), "2".into()]),
            ("bs".into(), vec!["32".into(), "64".into()]),
        ]);

        assert_eq!(combos.len(), 4);
        assert_eq!(combos[0].get("lr").map(String::as_str), Some("1"));
        assert_eq!(combos[0].get("bs").map(String::as_str), Some("32"));
        assert_eq!(combos[3].get("lr").map(String::as_str), Some("2"));
        assert_eq!(combos[3].get("bs").map(String::as_str), Some("64"));
    }
}
