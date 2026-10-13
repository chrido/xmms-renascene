//! Public-contract tests: these must work without egui or Android cfg gates.
use xmms_renascene::artwork::{
    normalize_track_identity, ArtworkKey, ArtworkSource, LatestArtworkQueue, Schedule,
    ARTWORK_RENDERER_VERSION, NOTIFICATION_ARTWORK_SIZE,
};

#[test]
fn public_artwork_contract_is_available_to_default_host_integration_tests() {
    let track = normalize_track_identity("file:///music/./song.wav", None);
    let key = ArtworkKey {
        normalized_track_identity: track.clone(),
        palette: [[0; 3]; 24],
        size: NOTIFICATION_ARTWORK_SIZE,
        renderer_version: ARTWORK_RENDERER_VERSION,
    };
    assert_eq!(key.spec().pixel_count(), Ok(320 * 320));
    let source = ArtworkSource {
        track_revision: 1,
        normalized_track_identity: track,
        skin_revision: 1,
        policy_revision: 1,
    };
    let mut queue = LatestArtworkQueue::new(1);
    let Schedule::Start(request) = queue.request(source.clone(), key).unwrap() else {
        panic!("first request did not start");
    };
    assert!(queue.finish(&request, &source, true).publish);
}

#[test]
fn public_renderer_needs_no_frontend_and_respects_an_all_black_palette() {
    let key = ArtworkKey {
        normalized_track_identity: "/music/black-skin.wav".into(),
        palette: [[0; 3]; 24],
        size: NOTIFICATION_ARTWORK_SIZE,
        renderer_version: ARTWORK_RENDERER_VERSION,
    };
    let image = xmms_renascene::render::artwork::render_artwork(&key.spec()).unwrap();
    assert_eq!((image.width(), image.height()), (320, 320));
    assert_eq!(image.pixels().len(), 320 * 320);
    assert!(image.pixels().iter().all(|pixel| *pixel == 0xff00_0000));
}
