use std::collections::HashMap;

const METHODS_WITH_REQUIRED_PROPS: &str = include_str!("../required_properties_info/required_props_info.csv");

pub(crate) type RequiredPropertiesMap = HashMap<&'static str, HashMap<&'static str, Vec<&'static str>>>;

// if we only need a specific sdk, maybe we could filter out the others?
// ideally this would be done at compile time, perhaps with konst crate
pub(crate) fn create_required_props_map() -> RequiredPropertiesMap {
    create_required_props_for(METHODS_WITH_REQUIRED_PROPS)
}

fn create_required_props_for(props: &'static str) -> RequiredPropertiesMap {
    let required_props_as_vec: Vec<(&str, &str, Vec<&str>)> = props
        .split('\n')
        .filter(|m| !m.is_empty())
        .map(|m| {
            let mut method_and_props: Vec<_> = m.split(',').collect();
            let required_props = method_and_props
                .pop()
                .expect("required props to be the third element")
                .split_whitespace()
                .collect();
            let method_name = method_and_props.pop().expect("method to be the second element");
            let service_name = method_and_props.pop().expect("service to be the first element");
            (service_name, method_name, required_props)
        })
        .collect();
    required_props_as_vec.into_iter().fold(
        HashMap::new(),
        |mut acc: HashMap<&'static str, HashMap<&'static str, Vec<&'static str>>>, (service_name, method_name, required_props)| {
            let map_for_method = acc.entry(method_name).or_default();
            map_for_method.entry(service_name).or_default().extend(required_props);
            acc
        },
    )
}

pub fn valid_sdks(required_props: &RequiredPropertiesMap, selected_sdks: &[String]) -> Result<(), String> {
    let service_names: Vec<_> = required_props.values()
        .flat_map(|v| v.keys())
        .collect();
    let not_found: Vec<String> = selected_sdks
        .iter()
        .map(|s| s.to_string())
        .filter(|s| !service_names.contains(&&s.as_ref()))
        .collect();

    if !not_found.is_empty() {
        Err(not_found.join(", "))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_required_props_creates_hashmap_with_entries_by_method_name_containing_hashmaps_by_service_key() {
        let props = "s3,write,bucket object\ns3,associate,account_arn\ns3control,associate,account_id identity_center_arn";

        let checks = create_required_props_for(props);

        assert_eq!(checks.keys().count(), 2);
        let write = checks.get("write").unwrap();
        let associate = checks.get("associate").unwrap();
        assert_eq!(write.keys().count(), 1);
        assert_eq!(write.get("s3"), Some(&vec!["bucket", "object"]));
        assert_eq!(associate.keys().count(), 2);
        assert_eq!(associate.get("s3"), Some(&vec!["account_arn"]));
        assert_eq!(associate.get("s3control"), Some(&vec!["account_id", "identity_center_arn"]));
    }

    #[test]
    fn test_not_present_in_required_props() {
        let mut required_props = HashMap::new();
        required_props.insert("something", HashMap::from([("s3", vec!["required_call"])]));
        required_props.insert("something_else", HashMap::from([("sqs", vec!["required_call"])]));

        let actual = valid_sdks(&required_props, &vec!["s3".to_string(), "sns".to_string()]).unwrap_err();

        assert_eq!(actual, "sns".to_string());
    }
}
