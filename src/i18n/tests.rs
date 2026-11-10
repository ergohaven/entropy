use super::{catalog_lookup, catalog_lookup_owned, index_catalog};

#[test]
fn embedded_catalogs_have_unique_supported_keys_and_readable_values() {
    for text in [
        include_str!("../../i18n/en.toml"),
        include_str!("../../i18n/ru.toml"),
    ] {
        let catalog = index_catalog(text);
        let mut keys = std::collections::HashSet::new();
        let mut section = "";
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].trim();
                continue;
            }
            let (name, _) = line.split_once('=').expect("catalog entry");
            let name = name.trim();
            assert!(!section.is_empty());
            assert!(!name.is_empty() && !name.contains('.'));
            let key = format!("{section}.{name}");
            assert!(keys.insert(key.clone()), "duplicate catalog key: {key}");
            assert!(
                catalog_lookup(&catalog, &key).is_some(),
                "unreadable value: {key}"
            );
            assert!(
                catalog_lookup_owned(&catalog, &key).is_some(),
                "invalid escapes: {key}"
            );
        }
    }
}

#[test]
fn indexed_catalog_keeps_sections_and_quoted_values() {
    let catalog = index_catalog(
        r#"
        # Embedded catalogs contain single-line quoted strings.
        [ first ]
        title = "Choose = select"
        empty = ''
        title = "Later duplicate"

        [second.nested]
        title = 'Выбрать'
        "#,
    );

    assert_eq!(
        catalog_lookup(&catalog, "first.title"),
        Some("Choose = select")
    );
    assert_eq!(catalog_lookup(&catalog, "first.empty"), Some(""));
    assert_eq!(
        catalog_lookup(&catalog, "second.nested.title"),
        Some("Выбрать")
    );
    assert_eq!(catalog_lookup(&catalog, "second.title"), None);
    assert_eq!(catalog_lookup_owned(&catalog, "first.missing"), None);
}

#[test]
fn indexed_catalog_preserves_borrowed_escapes_and_decodes_owned_strings() {
    let catalog = index_catalog(
        r#"
        [text]
        escaped = "line\nnext\r\t\"quote\"\\path"
        literal = 'line\nnext'
        "#,
    );

    assert_eq!(
        catalog_lookup(&catalog, "text.escaped"),
        Some(r#"line\nnext\r\t\"quote\"\\path"#),
    );
    assert_eq!(
        catalog_lookup_owned(&catalog, "text.escaped").as_deref(),
        Some("line\nnext\r\t\"quote\"\\path"),
    );
    assert_eq!(
        catalog_lookup(&catalog, "text.literal"),
        Some(r"line\nnext")
    );
    assert_eq!(
        catalog_lookup_owned(&catalog, "text.literal").as_deref(),
        Some(r"line\nnext"),
    );
}
