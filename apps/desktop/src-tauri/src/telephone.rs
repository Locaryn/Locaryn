//! L'app Android de cette version : son adresse, et un QR pour la
//! télécharger directement depuis le téléphone (mode simple).

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct AppTelephone {
    pub url: String,
    pub qr_svg: String,
}

/// L'APK publié avec la même version que cette app.
fn adresse_apk(version: &str) -> String {
    format!("https://github.com/Locaryn/Locaryn/releases/download/v{version}/locaryn-{version}-android.apk")
}

#[tauri::command]
pub async fn app_telephone() -> Result<AppTelephone, String> {
    let url = adresse_apk(env!("CARGO_PKG_VERSION"));
    let qr_svg = locaryn_travel::qr::svg(&url).map_err(|e| e.to_string())?;
    Ok(AppTelephone { url, qr_svg })
}

#[cfg(test)]
mod tests {
    #[test]
    fn l_apk_suit_le_nom_des_releases() {
        assert_eq!(
            super::adresse_apk("0.4.57"),
            "https://github.com/Locaryn/Locaryn/releases/download/v0.4.57/locaryn-0.4.57-android.apk"
        );
    }
}
