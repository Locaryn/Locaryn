//! Opens a real tunnel, checks the address comes back, closes it.
//!
//! Ignored: it needs one of the relays installed and configured, and it
//! briefly contacts a third party.
//!
//! It deliberately points at a port with nothing behind it. The tunnel is
//! what is under test; exposing a live server to run a test is not.
//!
//! ```text
//! LOCHOR_TEST_RELAY=ngrok cargo test -p lochor-travel --test live_tunnel -- --ignored --nocapture
//! ```
use lochor_travel::Provider;

#[tokio::test]
#[ignore = "needs a relay installed"]
async fn a_real_relay_hands_back_a_usable_address() {
    let name = std::env::var("LOCHOR_TEST_RELAY").expect("LOCHOR_TEST_RELAY");
    let provider = Provider::parse(&name).expect("relais inconnu");
    assert!(provider.is_available(), "{} n'est pas installé", provider.binary());

    // A closed port: nothing of this machine is published.
    let tunnel = lochor_travel::start(provider, 59_999)
        .await
        .expect("le relais n'a pas ouvert de tunnel");

    println!("adresse obtenue : {}", tunnel.url);
    assert!(tunnel.url.starts_with("https://"), "adresse non chiffrée");
    assert!(
        tunnel.url.matches('.').count() >= 2,
        "une adresse de relais a un sous-domaine : {}",
        tunnel.url
    );

    // And it must be signable and scannable, which is the whole chain.
    let dir = std::env::temp_dir().join("lochor_live_tunnel");
    std::fs::create_dir_all(&dir).unwrap();
    let ca = lochor_config::mtls::authority(&dir).unwrap();
    let now = 1_800_000_000;
    let uri = lochor_travel::sign(
        &ca.cert_pem,
        &ca.key_pem,
        lochor_travel::Mode::Travel,
        &tunnel.url,
        now,
        600,
    )
    .unwrap();
    let kid = lochor_travel::link::key_id(&ca.cert_pem).unwrap();
    let parsed =
        lochor_travel::verify(&uri, &|k| (k == kid).then(|| ca.cert_pem.clone()), now).unwrap();
    assert_eq!(parsed.url, tunnel.url);
    println!("lien signé et vérifié, {} caractères", uri.len());
    assert!(lochor_travel::qr::svg(&uri).is_ok(), "lien trop long pour un QR");

    tunnel.stop().await;
    std::fs::remove_dir_all(&dir).ok();
}
