//! Mise à jour de l'application Android.
//!
//! L'updater de Tauri ne couvre pas Android : il installe des paquets de
//! bureau. Sur un téléphone, une application distribuée hors magasin se met à
//! jour par le gestionnaire de paquets du système.
//!
//! Tout se passe donc ici, sans détour par un navigateur : l'application
//! compare sa version à celle publiée, dit d'où l'on part et où l'on va,
//! télécharge le paquet, puis ouvre l'installateur d'Android. Un paquet déjà
//! complet n'est jamais retéléchargé — c'est précisément le cas d'une
//! installation refusée faute d'autorisation, où reprendre ne doit rien coûter.
//!
//! Ce qui reste au système reste au système : c'est Android qui installe, qui
//! vérifie la signature et qui demande confirmation. Rien ne s'installe sans
//! que la personne ait vu son écran, et c'est très bien ainsi.

use futures::StreamExt as _;
use serde::Serialize;
use tauri::ipc::Channel;

/// Le manifeste que publie chaque release.
const MANIFEST_URL: &str =
    "https://github.com/Locaryn/locaryn/releases/latest/download/latest.json";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateStatus {
    /// Version installée sur ce téléphone.
    pub current: String,
    /// Dernière version publiée, quand on a pu la lire.
    pub latest: Option<String>,
    /// Vrai quand la version publiée est plus récente que celle installée.
    pub available: bool,
    /// Adresse de l'APK à installer.
    pub download_url: Option<String>,
    /// Ce que la version apporte, tel que la release le dit.
    pub notes: Option<String>,
    /// Taille de l'APK, pour une progression honnête et pour reconnaître un
    /// fichier déjà complet.
    pub size: Option<u64>,
    /// Vrai quand l'APK est déjà téléchargé et complet : il n'y a plus qu'à
    /// relancer l'installation, sans reprendre les trente mégaoctets.
    pub downloaded: bool,
    /// Ce qui a empêché la vérification, le cas échéant. Dit en français :
    /// c'est affiché tel quel.
    pub error: Option<String>,
}

impl UpdateStatus {
    fn unknown(error: impl Into<String>) -> Self {
        Self {
            current: env!("CARGO_PKG_VERSION").to_string(),
            latest: None,
            available: false,
            download_url: None,
            notes: None,
            size: None,
            downloaded: false,
            error: Some(error.into()),
        }
    }
}

/// Compare deux versions `x.y.z`.
///
/// Une comparaison de chaînes dirait que « 0.10.0 » précède « 0.9.0 » : il
/// faut comparer nombre par nombre. Ce qui n'est pas un nombre vaut zéro,
/// plutôt que de faire échouer la vérification sur une version exotique.
fn is_newer(latest: &str, current: &str) -> bool {
    fn parts(v: &str) -> Vec<u32> {
        v.trim_start_matches('v')
            .split(['.', '-', '+'])
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect()
    }
    let (a, b) = (parts(latest), parts(current));
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

/// Regarde s'il existe une version plus récente.
#[tauri::command]
pub async fn check_update(app: tauri::AppHandle) -> UpdateStatus {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => return UpdateStatus::unknown(format!("client réseau indisponible : {e}")),
    };

    let resp = match client.get(MANIFEST_URL).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            return UpdateStatus::unknown(format!(
                "le serveur de mises à jour a répondu {}",
                r.status()
            ))
        }
        Err(_) => {
            return UpdateStatus::unknown(
                "impossible de joindre le serveur de mises à jour — vérifiez la connexion",
            )
        }
    };

    let manifest: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return UpdateStatus::unknown(format!("manifeste illisible : {e}")),
    };

    let latest = manifest["version"].as_str().unwrap_or_default().to_string();
    let url = manifest["platforms"]["android"]["url"]
        .as_str()
        .map(str::to_string);
    let size = manifest["platforms"]["android"]["size"].as_u64();
    let notes = manifest["notes"]
        .as_str()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string);

    // Une version plus récente sans fichier à installer n'est pas une mise à
    // jour : c'est une promesse qu'on ne pourrait pas tenir.
    let available = !latest.is_empty() && is_newer(&latest, &current) && url.is_some();

    // Un APK déjà téléchargé en entier n'a pas à l'être une seconde fois :
    // c'est le cas quand une installation a été refusée puis reprise.
    //
    // Le fichier ne prend son nom définitif qu'une fois complet — il se
    // télécharge sous `.part`. Sa présence suffit donc à répondre, et la
    // taille, quand le manifeste la donne, confirme.
    let downloaded = url
        .as_deref()
        .and_then(|u| fichier_apk(&app, u))
        .and_then(|p| std::fs::metadata(p).ok())
        .is_some_and(|m| size.is_none_or(|t| m.len() == t));

    // Plus de mise à jour en attente : le paquet installé la veille n'a plus
    // de raison d'occuper trente mégaoctets sur le téléphone.
    if !available {
        vider_les_paquets(&app);
    }

    UpdateStatus {
        current,
        latest: (!latest.is_empty()).then_some(latest),
        available,
        download_url: url,
        notes,
        size,
        downloaded,
        error: None,
    }
}

/// Où se range l'APK téléchargé.
///
/// Le dossier de cache, parce que c'est celui que le fournisseur de fichiers
/// déclare : c'est de là qu'Android accepte de lire le paquet à installer.
fn fichier_apk(app: &tauri::AppHandle, url: &str) -> Option<std::path::PathBuf> {
    use tauri::Manager as _;
    let nom = url.rsplit('/').next().filter(|n| n.ends_with(".apk"))?;
    let dir = app.path().app_cache_dir().ok()?.join("updates");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join(nom))
}

/// Effacer les paquets téléchargés.
///
/// Appelé quand il n'y a plus rien à installer : soit la mise à jour a été
/// faite, soit elle n'a jamais existé. Dans les deux cas, garder l'APK ne sert
/// qu'à remplir le téléphone.
fn vider_les_paquets(app: &tauri::AppHandle) {
    use tauri::Manager as _;
    let Ok(dir) = app.path().app_cache_dir() else {
        return;
    };
    let dir = dir.join("updates");
    let Ok(entrees) = std::fs::read_dir(&dir) else {
        return;
    };
    for entree in entrees.flatten() {
        let _ = std::fs::remove_file(entree.path());
    }
}

/// Un point d'avancement du téléchargement, envoyé au fil et à mesure.
///
/// `percentage` reste `None` quand le manifeste n'a pas donné de taille : un
/// octet compté ne dit rien sans un total à côté, et afficher une barre figée
/// à un pourcentage inventé mentirait plus qu'une barre indéterminée.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProgressionTelechargement {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub percentage: Option<u8>,
}

/// Télécharger la nouvelle version, puis la confier à l'installateur.
///
/// L'application ne renvoie plus vers une page web : elle télécharge, puis
/// ouvre le paquet. C'est Android qui installe, vérifie la signature et
/// demande confirmation — cette part-là n'est pas négociable, et c'est très
/// bien ainsi.
///
/// Un APK déjà complet n'est pas retéléchargé. C'est exactement le cas d'une
/// installation refusée faute d'autorisation : on relance l'installateur, on
/// ne reprend pas trente mégaoctets.
#[tauri::command]
pub async fn install_update(
    app: tauri::AppHandle,
    url: String,
    size: Option<u64>,
    on_progress: Channel<ProgressionTelechargement>,
) -> Result<String, String> {
    if !url.starts_with("https://github.com/Locaryn/") {
        return Err("adresse de mise à jour inattendue".to_string());
    }
    let cible = fichier_apk(&app, &url).ok_or("dossier de téléchargement indisponible")?;

    // Le paquet ne prend son nom définitif qu'une fois entier : le trouver
    // suffit. La taille, quand le manifeste la donne, confirme.
    let deja_complet = std::fs::metadata(&cible)
        .map(|m| size.is_none_or(|t| m.len() == t))
        .unwrap_or(false);

    if !deja_complet {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|_| "téléchargement impossible — vérifiez la connexion".to_string())?;
        if !resp.status().is_success() {
            return Err(format!("le serveur a répondu {}", resp.status()));
        }
        // La réponse elle-même connaît sa longueur, quand le serveur l'a
        // annoncée ; elle ne dépend pas du manifeste, qui peut mentir ou dater.
        let total = resp.content_length().or(size);

        // Écriture d'abord à côté, puis renommage : un fichier partiel ne doit
        // jamais passer pour un paquet complet si le téléchargement casse.
        let partiel = cible.with_extension("apk.part");
        let mut fichier =
            std::fs::File::create(&partiel).map_err(|e| format!("écriture : {e}"))?;

        let mut recu: u64 = 0;
        let mut flux = resp.bytes_stream();
        // Un octet par octet suffirait à saturer le pont IPC ; regrouper les
        // envois à un seuil fixe garde la barre fluide sans le noyer.
        let mut prochain_seuil: u64 = 0;
        const PAS_MO: u64 = 256 * 1024;
        while let Some(morceau) = flux.next().await {
            let morceau = morceau.map_err(|e| format!("téléchargement interrompu : {e}"))?;
            std::io::Write::write_all(&mut fichier, &morceau)
                .map_err(|e| format!("écriture : {e}"))?;
            recu += morceau.len() as u64;
            if recu >= prochain_seuil {
                prochain_seuil = recu + PAS_MO;
                let _ = on_progress.send(ProgressionTelechargement {
                    downloaded: recu,
                    total,
                    percentage: total
                        .filter(|&t| t > 0)
                        .map(|t| ((recu as f64 / t as f64) * 100.0).min(100.0) as u8),
                });
            }
        }
        drop(fichier);
        // Le dernier point, toujours envoyé : sans lui, une taille mal connue
        // à l'avance (seuil jamais atteint sur un petit reste) laisserait la
        // barre en dessous de 100 % alors que le fichier est déjà complet.
        let _ = on_progress.send(ProgressionTelechargement {
            downloaded: recu,
            total,
            percentage: total.filter(|&t| t > 0).map(|_| 100),
        });
        std::fs::rename(&partiel, &cible).map_err(|e| format!("renommage : {e}"))?;
    }

    ouvrir_le_paquet(&app, &cible)?;
    Ok(cible.to_string_lossy().to_string())
}

/// Reprendre une installation refusée, sans retélécharger.
#[tauri::command]
pub async fn resume_install(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let cible = fichier_apk(&app, &url).ok_or("aucun paquet téléchargé")?;
    if !cible.is_file() {
        return Err("le paquet n'est plus là — relancez le téléchargement".into());
    }
    ouvrir_le_paquet(&app, &cible)
}

fn ouvrir_le_paquet(app: &tauri::AppHandle, chemin: &std::path::Path) -> Result<(), String> {
    let _ = app;
    #[cfg(target_os = "android")]
    {
        return lancer_installateur(chemin);
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = chemin;
        Err("l'installation d'un APK n'a de sens que sur un téléphone".into())
    }
}

/// Ouvrir l'installateur d'Android sur un paquet local.
///
/// Il faut construire l'`Intent` à la main. Le greffon `opener` ne sait ouvrir
/// qu'une adresse, et passer par un navigateur n'est pas une mise à jour :
/// c'est un détour qui laisse la personne se débrouiller avec un fichier
/// téléchargé.
///
/// Deux choses se jouent ici. D'abord l'autorisation : depuis Android 8, une
/// application doit avoir le droit d'en installer d'autres, et ce droit se
/// donne écran par écran. Plutôt que de dire d'aller le chercher, on ouvre cet
/// écran-là. Ensuite le fichier : il est confié au fournisseur déclaré par
/// l'application, parce qu'Android refuse depuis longtemps qu'une application
/// en expose une autre un `file://` — et c'est une bonne chose.
///
/// C'est ensuite le système qui installe, vérifie la signature et demande
/// confirmation.
#[cfg(target_os = "android")]
fn lancer_installateur(chemin: &std::path::Path) -> Result<(), String> {
    use jni::objects::{JObject, JValue};
    use tao::platform::android::prelude::main_android_context;

    const FLAG_GRANT_READ: i32 = 0x0000_0001;
    const FLAG_NEW_TASK: i32 = 0x1000_0000;

    let ctx = main_android_context().ok_or("activité Android introuvable")?;
    let vm = unsafe { jni::JavaVM::from_raw(ctx.java_vm.cast()) }
        .map_err(|e| format!("machine virtuelle indisponible : {e}"))?;
    let activite = unsafe { JObject::from_raw(ctx.context_jobject.cast()) };
    // En démon : ce fil appartient à tokio, et le détachement de JNI ne doit
    // pas dépendre de sa fin de vie.
    let mut env = vm
        .attach_current_thread_as_daemon()
        .map_err(|e| format!("attachement impossible : {e}"))?;

    let chemin_txt = chemin.to_string_lossy().to_string();

    // A-t-on le droit d'installer ? Sinon, on ouvre l'écran qui le donne.
    let mut autorise = || -> Result<bool, jni::errors::Error> {
        let pm = env
            .call_method(
                &activite,
                "getPackageManager",
                "()Landroid/content/pm/PackageManager;",
                &[],
            )?
            .l()?;
        env.call_method(&pm, "canRequestPackageInstalls", "()Z", &[])?
            .z()
    };
    let permis = autorise().unwrap_or_else(|_| {
        let _ = env.exception_clear();
        // Avant Android 8 la question ne se posait pas : on tente.
        true
    });

    let mut demander_le_droit = || -> Result<(), jni::errors::Error> {
        let action = env.new_string("android.settings.MANAGE_UNKNOWN_APP_SOURCES")?;
        let paquet = nom_du_paquet(&mut env, &activite)?;
        let cible = env.new_string(format!("package:{paquet}"))?;
        let uri = env
            .call_static_method(
                "android/net/Uri",
                "parse",
                "(Ljava/lang/String;)Landroid/net/Uri;",
                &[(&cible).into()],
            )?
            .l()?;
        let intent = env.new_object(
            "android/content/Intent",
            "(Ljava/lang/String;Landroid/net/Uri;)V",
            &[(&action).into(), (&uri).into()],
        )?;
        env.call_method(
            &intent,
            "addFlags",
            "(I)Landroid/content/Intent;",
            &[JValue::Int(FLAG_NEW_TASK)],
        )?;
        env.call_method(
            &activite,
            "startActivity",
            "(Landroid/content/Intent;)V",
            &[(&intent).into()],
        )?;
        Ok(())
    };

    if !permis {
        let ouvert = demander_le_droit().is_ok();
        let _ = env.exception_clear();
        return Err(if ouvert {
            "Locaryn n'a pas encore le droit d'installer des applications. L'écran \
             d'autorisation vient de s'ouvrir : accordez-le, revenez, et reprenez — le \
             paquet est déjà téléchargé."
                .to_string()
        } else {
            "Locaryn n'a pas le droit d'installer des applications. Accordez-le dans \
             Réglages → Applications → Locaryn → Installer des applications inconnues, \
             puis reprenez : le paquet est déjà téléchargé."
                .to_string()
        });
    }

    let mut travail = || -> Result<(), jni::errors::Error> {
        // File(chemin)
        let s_chemin = env.new_string(&chemin_txt)?;
        let fichier = env.new_object(
            "java/io/File",
            "(Ljava/lang/String;)V",
            &[(&s_chemin).into()],
        )?;

        // FileProvider.getUriForFile(activité, "<paquet>.fileprovider", fichier)
        let paquet = nom_du_paquet(&mut env, &activite)?;
        let autorite = env.new_string(format!("{paquet}.fileprovider"))?;
        let classe = classe_de_l_app(&mut env, &activite, "androidx.core.content.FileProvider")?;
        let uri = env
            .call_static_method(
                &classe,
                "getUriForFile",
                "(Landroid/content/Context;Ljava/lang/String;Ljava/io/File;)Landroid/net/Uri;",
                &[(&activite).into(), (&autorite).into(), (&fichier).into()],
            )?
            .l()?;

        // Intent(ACTION_VIEW).setDataAndType(uri, mime du paquet)
        let action = env.new_string("android.intent.action.VIEW")?;
        let intent = env.new_object(
            "android/content/Intent",
            "(Ljava/lang/String;)V",
            &[(&action).into()],
        )?;
        let mime = env.new_string("application/vnd.android.package-archive")?;
        env.call_method(
            &intent,
            "setDataAndType",
            "(Landroid/net/Uri;Ljava/lang/String;)Landroid/content/Intent;",
            &[(&uri).into(), (&mime).into()],
        )?;
        // Lire l'URI est un droit qu'on accorde à l'installateur, et à lui
        // seul, le temps de l'installation.
        env.call_method(
            &intent,
            "addFlags",
            "(I)Landroid/content/Intent;",
            &[JValue::Int(FLAG_GRANT_READ | FLAG_NEW_TASK)],
        )?;

        env.call_method(
            &activite,
            "startActivity",
            "(Landroid/content/Intent;)V",
            &[(&intent).into()],
        )?;
        Ok(())
    };

    if travail().is_ok() {
        return Ok(());
    }

    // « Java exception was thrown » ne dit rien à personne. On va chercher le
    // message que la machine virtuelle a mis de côté.
    Err(format!(
        "Android n'a pas ouvert l'installateur : {}. Le paquet est déjà téléchargé — \
         reprendre ne le retéléchargera pas.",
        message_de_l_exception(&mut env)
    ))
}

/// Trouver une classe **de l'application**, pas seulement du système.
///
/// Un fil natif n'hérite pas du chargeur de classes de l'application : demander
/// `androidx.core.content.FileProvider` à JNI depuis ici répond
/// `ClassNotFoundException`, alors que la classe est bien dans l'APK. L'activité
/// expose `getAppClass` exactement pour cette raison — c'est elle qui connaît le
/// bon chargeur.
#[cfg(target_os = "android")]
fn classe_de_l_app<'a>(
    env: &mut jni::JNIEnv<'a>,
    activite: &jni::objects::JObject,
    nom: &str,
) -> Result<jni::objects::JClass<'a>, jni::errors::Error> {
    let nom = env.new_string(nom)?;
    let classe = env
        .call_method(
            activite,
            "getAppClass",
            "(Ljava/lang/String;)Ljava/lang/Class;",
            &[(&nom).into()],
        )?
        .l()?;
    Ok(classe.into())
}

/// Le nom du paquet de l'application.
#[cfg(target_os = "android")]
fn nom_du_paquet(
    env: &mut jni::JNIEnv,
    activite: &jni::objects::JObject,
) -> Result<String, jni::errors::Error> {
    let nom = env
        .call_method(activite, "getPackageName", "()Ljava/lang/String;", &[])?
        .l()?;
    let nom: jni::objects::JString = nom.into();
    let texte = env.get_string(&nom)?;
    Ok(texte.into())
}

/// Ce que l'exception Java en attente raconte, en clair.
///
/// Sans cela, l'écran affiche « Java exception was thrown », qui n'aide ni la
/// personne devant le téléphone ni celle qui lit le rapport.
#[cfg(target_os = "android")]
fn message_de_l_exception(env: &mut jni::JNIEnv) -> String {
    let Ok(exception) = env.exception_occurred() else {
        return "cause inconnue".to_string();
    };
    // Il faut effacer avant de rappeler quoi que ce soit : tant qu'une
    // exception est en attente, JNI refuse tout autre appel.
    let _ = env.exception_clear();
    let texte = env
        .call_method(&exception, "toString", "()Ljava/lang/String;", &[])
        .and_then(|v| v.l())
        .and_then(|s| {
            env.get_string((&s).into())
                .map(String::from)
                .map_err(Into::into)
        });
    let _ = env.exception_clear();
    texte.unwrap_or_else(|_| "cause inconnue".to_string())
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn les_versions_se_comparent_nombre_par_nombre() {
        assert!(is_newer("0.10.0", "0.9.0"), "0.10 vient après 0.9");
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(
            !is_newer("0.2.2", "0.2.2"),
            "la même version n'en est pas une nouvelle"
        );
        assert!(!is_newer("0.2.1", "0.2.2"));
        assert!(is_newer("v0.3.0", "0.2.9"), "le v du tag est toléré");
    }
}
