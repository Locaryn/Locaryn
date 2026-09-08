fn main() {
    tauri_build::build();
    manifeste_des_tests();
}

/// Le nom de la variable qui réclame le manifeste des tests.
const DEMANDE: &str = "LOCARYN_TEST_MANIFEST";

/// Donne au binaire de test le manifeste que Windows exige — sur demande.
///
/// # Le problème
///
/// `cargo test -p locaryn-desktop --lib` échoue sous Windows avant d'exécuter
/// le moindre test, avec `STATUS_ENTRYPOINT_NOT_FOUND` (0xC0000139) et aucune
/// explication. La cause : l'exécutable de test lie les DLL d'interface —
/// `comctl32` parmi elles, apportée par les dialogues natifs — et y importe
/// `TaskDialogIndirect`, qui n'existe qu'en version 6. Sans manifeste
/// réclamant cette version, le chargeur fournit la version 5 de
/// `System32\comctl32.dll`, l'import ne se résout pas, et le processus meurt
/// au démarrage. Conséquence : les tests de cette coque ne tournaient qu'en
/// intégration continue, sur Linux.
///
/// `tauri_build::build()` pose bien ce manifeste, mais sur l'exécutable de
/// l'application seulement (`rustc-link-arg-bins`) : le harnais de test est
/// une autre cible, et il n'en hérite pas.
///
/// # Pourquoi une variable d'environnement
///
/// Aucune instruction de script de compilation ne cible les tests unitaires
/// d'une bibliothèque : `rustc-link-arg-tests` ne vaut que pour les cibles
/// déclarées dans `tests/`, et `rustc-link-arg` s'appliquerait aussi au
/// binaire de l'application — qui reçoit déjà son manifeste par
/// `resource.lib`. Les deux ensemble donnent `CVT1100 : ressource en double`,
/// puis `LNK1123` : l'application ne se lie plus du tout.
///
/// D'où le choix explicite. Sous Windows, pour lancer les tests de cette
/// coque :
///
/// ```text
/// LOCARYN_TEST_MANIFEST=1 cargo test -p locaryn-desktop --lib
/// ```
///
/// La variable ne doit pas être posée pour compiler l'application, ni pour un
/// `cargo test` qui assemble une cible binaire — `cargo test -p locaryn-desktop`
/// sans `--lib`, ou `cargo test --workspace` : le lien y échoue sur `CVT1100`
/// puis `LNK1123`, le binaire ayant déjà son manifeste. C'est pourquoi elle
/// n'est pas posée d'office, et pourquoi sa présence déclenche un
/// avertissement à chaque compilation.
///
/// L'intégration continue tourne sous Linux, où rien de tout cela n'existe :
/// elle ne pose pas la variable, et exécute les mêmes tests sans elle.
fn manifeste_des_tests() {
    println!("cargo::rerun-if-env-changed={DEMANDE}");
    if std::env::var(DEMANDE).is_err() {
        return;
    }
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => "amd64",
        Ok("aarch64") => "arm64",
        Ok("x86") => "x86",
        // Une architecture inconnue : on ne devine pas. Sans manifeste, le
        // test échouera comme avant — pas plus mal qu'un manifeste faux, qui
        // ferait échouer le lien lui-même.
        _ => return,
    };
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let chemin = out.join("tests.manifest");
    let manifeste = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0" processorArchitecture="{arch}"
        publicKeyToken="6595b64144ccf1df" language="*" />
    </dependentAssembly>
  </dependency>
</assembly>
"#
    );
    if let Err(e) = std::fs::write(&chemin, manifeste) {
        // On n'interrompt pas la compilation pour un manifeste : sans lui, le
        // binaire de l'application reste correct, et seuls les tests locaux
        // retrouvent leur ancien échec.
        println!("cargo::warning=manifeste des tests non écrit : {e}");
        return;
    }
    // Bruyant, parce que le piege est reel : `rustc-link-arg` touche toutes
    // les cibles, y compris la cible de test du *binaire*, qui recoit deja son
    // manifeste par `resource.lib`. Un `cargo test -p locaryn-desktop` sans
    // `--lib` echoue alors sur `CVT1100` puis `LNK1123` — mesure. Mieux vaut
    // l'avertissement a chaque compilation que la surprise au lien.
    println!(
        "cargo::warning={DEMANDE} est pose : n'assemblez que la bibliotheque, \
         `cargo test -p locaryn-desktop --lib`. Toute cible binaire echouera au lien."
    );
    println!("cargo::rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo::rustc-link-arg=/MANIFESTINPUT:{}", chemin.display());
}
