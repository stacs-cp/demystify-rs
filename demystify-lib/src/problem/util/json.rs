use serde_json::Value;

fn merge_into_serde_json_dict_impl(a: &mut Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(ref mut a), Value::Object(ref b)) => {
            for (k, v) in b {
                a.insert(k.clone(), v.clone());
            }
            true
        }
        (_, _) => false,
    }
}

pub fn merge_into_serde_json_dict(a: &mut Value, b: &Value) {
    if !merge_into_serde_json_dict_impl(a, b) {
        panic!("merging non-dictionaries: {} {}", a, b);
    }
}

#[cfg(test)]
mod tests {

    use serde_json::Value;

    use crate::problem::util::json::merge_into_serde_json_dict;

    #[test]
    fn test_merge_json_dicts() {
        // Test case 1: Merging two empty dictionaries
        let mut a: Value = serde_json::from_str("{}").unwrap();
        let b: Value = serde_json::from_str("{}").unwrap();
        let expected: Value = serde_json::from_str("{}").unwrap();
        merge_into_serde_json_dict(&mut a, &b);
        assert_eq!(a, expected);
    }

    #[test]
    fn test_merge_json_dicts_2() {
        // Test case 2: Merging two dictionaries with overlapping keys
        let mut a: Value = serde_json::from_str(r#"{"a": 1, "b": 2}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"b": 3, "c": 4}"#).unwrap();
        let expected: Value = serde_json::from_str(r#"{"a": 1, "b": 3, "c": 4}"#).unwrap();
        merge_into_serde_json_dict(&mut a, &b);
        assert_eq!(a, expected);
    }

    #[test]
    fn test_merge_json_dicts_3() {
        // Test case 3: Merging a dictionary with a non-dictionary value
        let a: Value = serde_json::from_str(r#"{"a": 1}"#).unwrap();
        let b: Value = serde_json::from_str(r#"2"#).unwrap();
        assert!(std::panic::catch_unwind(|| {
            let mut a = a.clone();
            let b = b.clone();
            merge_into_serde_json_dict(&mut a, &b)
        })
        .is_err());
    }
}
