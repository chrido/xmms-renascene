use super::*;

#[test]
fn missing_policy_defaults_to_auto_and_invalid_values_fail_closed() {
    assert_eq!(ArtworkPolicy::default(), ArtworkPolicy::Auto);
    assert_eq!(Config::default().artwork_policy, ArtworkPolicy::Auto);
    for input in [
        "",
        "[xmms]\nskin=/classic\n",
        "[other]\nartwork_policy=generated\n",
    ] {
        assert_eq!(
            Config::from_key_file_str(input).artwork_policy,
            ArtworkPolicy::Auto
        );
    }
    for input in [
        "[xmms]\nartwork_policy=\n",
        "[xmms]\nartwork_policy=1\n",
        "[xmms]\nartwork_policy=nonsense\n",
    ] {
        assert_eq!(
            Config::from_key_file_str(input).artwork_policy,
            ArtworkPolicy::Off
        );
    }
}

#[test]
fn generated_policy_accepts_case_and_round_trips_with_skin() {
    for value in ["generated", "GENERATED", "Generated", "  generated  "] {
        let input = format!("[xmms]\nskin=/classic\nartwork_policy={value}\n");
        let config = Config::from_key_file_str(&input);
        assert_eq!(config.artwork_policy, ArtworkPolicy::Generated);
        let serialized = config.to_key_file_string();
        assert!(serialized.contains("artwork_policy=generated\n"));
        assert_eq!(Config::from_key_file_str(&serialized), config);
    }
}

#[test]
fn auto_policy_round_trips_as_the_default() {
    let config = Config::from_key_file_str("[xmms]\nartwork_policy=auto\n");
    assert_eq!(config.artwork_policy, ArtworkPolicy::Auto);
    assert_eq!(
        Config::from_key_file_str(&config.to_key_file_string()),
        config
    );
    assert_eq!(Config::default().artwork_policy, ArtworkPolicy::Auto);
}

#[test]
fn audio_inspired_policy_round_trips_with_skin_and_preserves_default() {
    for value in ["audio-inspired", "AUDIO-INSPIRED", " Audio-Inspired "] {
        let config =
            Config::from_key_file_str(&format!("[xmms]\nskin=/classic\nartwork_policy={value}\n"));
        assert_eq!(config.artwork_policy, ArtworkPolicy::AudioInspired);
        assert_eq!(
            Config::from_key_file_str(&config.to_key_file_string()),
            config
        );
        assert_eq!(config.skin.as_deref(), Some("/classic"));
    }
    assert_eq!(Config::default().artwork_policy, ArtworkPolicy::Auto);
}

#[test]
fn off_policy_round_trips_without_palette_snapshot_keys() {
    let config = Config {
        skin: Some("/classic".into()),
        artwork_policy: ArtworkPolicy::Off,
        ..Config::default()
    };
    let serialized = config.to_key_file_string();
    assert!(serialized.contains("artwork_policy=off\n"));
    assert!(!serialized.contains("palette"));
    assert_eq!(Config::from_key_file_str(&serialized), config);
}

#[test]
fn application_persistence_snapshot_keeps_artwork_policy() {
    let config = Config {
        artwork_policy: ArtworkPolicy::Generated,
        skin: Some("/classic".into()),
        ..Config::default()
    };
    let state = crate::app_state::AppState::from_config(config.clone());
    let snapshot = state.persistence_snapshot();
    assert_eq!(snapshot.config.artwork_policy, ArtworkPolicy::Generated);
    assert_eq!(snapshot.config.skin, config.skin);
}
