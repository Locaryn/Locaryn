//! Actualisation réelle du catalogue, sur le réseau. Ignorée par défaut :
//!
//! cargo test -p locaryn-extensions --test catalogue_reel -- --ignored --nocapture
//!
//! `LOCARYN_TEST_CATALOG_CACHE=<extension-catalog.json>` part d'un cache
//! existant : de quoi éprouver le repli quand le quota GitHub est épuisé.

use locaryn_extensions::catalog::{builtin_sources, CatalogClient};

#[tokio::test]
#[ignore = "lit les catalogues sur le réseau"]
async fn l_actualisation_tient_en_quelques_secondes() {
    let cache = std::env::temp_dir().join("locaryn-catalogue-reel.json");
    if let Ok(depart) = std::env::var("LOCARYN_TEST_CATALOG_CACHE") {
        std::fs::copy(&depart, &cache).expect("cache de départ illisible");
    }
    let client = CatalogClient::with_cache_path(reqwest::Client::new(), cache.clone());
    let debut = std::time::Instant::now();
    let snap = client.refresh(&builtin_sources()).await;
    let duree = debut.elapsed();
    for s in &snap.sources {
        println!(
            "{:<40} ok={} entrées={} {}",
            s.source.label,
            s.ok,
            s.entry_count,
            s.error.clone().unwrap_or_default()
        );
    }
    println!(
        "{} entrées en {:.1} s",
        snap.entries.len(),
        duree.as_secs_f32()
    );
    if let Err(e) = std::fs::remove_file(&cache) {
        eprintln!("cache d'essai non effacé : {e}");
    }
    assert!(snap.entries.iter().any(|e| e.name.starts_with("morph-")));
    assert!(duree.as_secs() < 15, "actualisation trop lente : {duree:?}");
}

/// Le temps de chaque source seule : c'est la plus lente qui fixe la durée.
#[tokio::test]
#[ignore = "lit les catalogues sur le réseau"]
async fn duree_de_chaque_source() {
    let cache = std::env::temp_dir().join("locaryn-catalogue-reel-source.json");
    let client = CatalogClient::with_cache_path(reqwest::Client::new(), cache.clone());
    for source in builtin_sources() {
        let debut = std::time::Instant::now();
        let snap = client.refresh(std::slice::from_ref(&source)).await;
        println!(
            "{:<40} {:>5} entrées en {:.1} s",
            source.label,
            snap.entries.len(),
            debut.elapsed().as_secs_f32()
        );
    }
    if let Err(e) = std::fs::remove_file(&cache) {
        eprintln!("cache d'essai non effacé : {e}");
    }
}
