//! One bare address for hosted account identity, rate limits and delivery.

pub(crate) fn normalize(input: &str) -> Option<String> {
    let email = input.trim().to_ascii_lowercase();
    if email.len() > 254
        || !email.bytes().all(|b| b.is_ascii_graphic())
        || email.contains(['<', '>', '(', ')', ',', ';', ':', '\"', '\\'])
        || email.parse::<lettre::Address>().is_err()
    {
        return None;
    }
    Some(email)
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn only_one_bare_address_is_accepted() {
        assert_eq!(
            normalize(" Alice+tag@Example.test ").as_deref(),
            Some("alice+tag@example.test")
        );
        for input in [
            "a<victim@example.test>",
            "victim@example.test (name)",
            "victim@example.test,other@example.test",
            "victim@example.test;other@example.test",
            "\"victim\"@example.test",
            "victim@@example.test",
            "victim@",
            "@example.test",
            "victim@example.test\r\nBcc:other@example.test",
        ] {
            assert!(normalize(input).is_none(), "accepted {input:?}");
        }
    }
}
