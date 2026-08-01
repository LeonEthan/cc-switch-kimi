use std::str::FromStr;

use cc_switch_lib::AppType;

#[test]
fn parse_known_apps_case_insensitive_and_trim() {
    assert!(matches!(AppType::from_str("claude"), Ok(AppType::Claude)));
    assert!(matches!(AppType::from_str("codex"), Ok(AppType::Codex)));
    assert!(matches!(
        AppType::from_str("grokbuild"),
        Ok(AppType::GrokBuild)
    ));
    assert!(matches!(
        AppType::from_str("Grok-Build"),
        Ok(AppType::GrokBuild)
    ));
    assert!(matches!(
        AppType::from_str(" ClAuDe \n"),
        Ok(AppType::Claude)
    ));
    assert!(matches!(AppType::from_str("\tcoDeX\t"), Ok(AppType::Codex)));
    assert!(matches!(
        AppType::from_str("kimicode"),
        Ok(AppType::KimiCode)
    ));
    assert!(matches!(
        AppType::from_str("kimi-code"),
        Ok(AppType::KimiCode)
    ));
    assert!(matches!(AppType::from_str("kimi"), Ok(AppType::KimiCode)));
    assert!(matches!(AppType::from_str("omp"), Ok(AppType::Omp)));
    assert!(matches!(AppType::from_str("oh-my-pi"), Ok(AppType::Omp)));
    assert!(matches!(AppType::from_str(" Omp \n"), Ok(AppType::Omp)));
}

#[test]
fn parse_unknown_app_returns_localized_error_message() {
    let err = AppType::from_str("unknown").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("可选值") || msg.contains("Allowed"));
    assert!(msg.contains("unknown"));
}
