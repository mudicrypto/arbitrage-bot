use mm_backtest::campaign::{Config, bytes_under, inventory, resume};
#[tokio::test]
async fn completed_campaign_resumes_without_overwriting_any_session() {
    let dir = tempfile::tempdir().unwrap();
    let mut c: Config =
        serde_json::from_str(include_str!("../../../config/collection_campaign.json")).unwrap();
    c.root = dir.path().join("campaign");
    let session = c.root.join("sessions/session-old");
    std::fs::create_dir_all(&session).unwrap();
    let marker = br#"{"reason":"duration_complete","observed_seconds":86400}"#;
    std::fs::write(session.join("collection_finished.json"), marker).unwrap();
    let config = dir.path().join("config.json");
    std::fs::write(&config, serde_json::to_vec(&c).unwrap()).unwrap();
    resume(&config, None).await.unwrap();
    resume(&config, None).await.unwrap();
    assert_eq!(inventory(&c.root.join("sessions")).unwrap().len(), 1);
    assert_eq!(
        std::fs::read(session.join("collection_finished.json")).unwrap(),
        marker
    );
    assert!(bytes_under(&c.root).unwrap() < c.max_campaign_bytes);
}
#[tokio::test]
async fn unfinished_session_is_never_taken_over_or_corrupted() {
    let dir = tempfile::tempdir().unwrap();
    let mut c: Config =
        serde_json::from_str(include_str!("../../../config/collection_campaign.json")).unwrap();
    c.root = dir.path().join("campaign");
    let session = c.root.join("sessions/session-live");
    std::fs::create_dir_all(&session).unwrap();
    std::fs::write(session.join("status.json"), b"untouched").unwrap();
    let config = dir.path().join("config.json");
    std::fs::write(&config, serde_json::to_vec(&c).unwrap()).unwrap();
    assert!(resume(&config, None).await.is_err());
    assert_eq!(
        std::fs::read(session.join("status.json")).unwrap(),
        b"untouched"
    );
    assert_eq!(inventory(&c.root.join("sessions")).unwrap().len(), 1);
}
