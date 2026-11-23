//! ORDiDOC : au lancement de SOS ORDiDOC, annonce de l'identifiant du poste a
//! CODI, pour que le technicien retrouve le poste dans le ticket du client.
//!
//! Seuls l'identifiant et le nom du poste sont transmis, jamais le mot de
//! passe de session. Le lien d'un ticket nomme le telechargement
//! `SOS-ORDiDOC-<jeton>.exe` : le jeton, lu dans le nom du fichier, rattache
//! l'annonce au ticket. Sans jeton, CODI ne reconnait qu'un poste deja connu.
//! Tout echec (pas de reseau, serveur injoignable) est silencieux pour le
//! client : SOS ORDiDOC fonctionne comme avant.

use hbb_common::{bail, config::Config, log, ResultType};
use std::time::{Duration, Instant};

const URL_ANNONCE: &str = "https://suivi.ordidoc.fr/api/public/sos/annonce";
const LONGUEUR_JETON: usize = 12;
/// Laisse au client le temps de s'enregistrer aupres du serveur de rendez-vous.
const ATTENTE_INITIALE: Duration = Duration::from_secs(5);
const ATTENTE_MAX: Duration = Duration::from_secs(300);
const PAS: Duration = Duration::from_secs(2);
const DELAI_REQUETE: Duration = Duration::from_secs(15);
const ESSAIS: u64 = 3;

fn est_variante_sos() -> bool {
    option_env!("ORDIDOC_VARIANT") == Some("sos")
}

/// Jeton du lien de ticket dans le nom du fichier : le premier morceau de
/// douze caracteres alphanumeriques (`SOS-ORDiDOC-Ab3xK9mP2qRs.exe`,
/// `SOS-ORDiDOC-Ab3xK9mP2qRs (1).exe`). Aucun pour le nom d'origine.
fn jeton_du_nom(nom: &str) -> Option<String> {
    let fichier = nom.rsplit(|c| c == '/' || c == '\\').next().unwrap_or(nom);
    fichier
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find(|morceau| morceau.len() == LONGUEUR_JETON)
        .map(|morceau| morceau.to_owned())
}

/// Nom du fichier lance par le client : le lanceur portable le transmet au
/// programme extrait ; a defaut, le nom du programme lui-meme.
fn nom_d_origine() -> String {
    if let Ok(nom) = std::env::var(crate::common::PORTABLE_APPNAME_RUNTIME_ENV_KEY) {
        if !nom.is_empty() {
            return nom;
        }
    }
    std::env::current_exe()
        .ok()
        .and_then(|chemin| chemin.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

/// L'identifiant n'est fiable qu'une fois accepte par le serveur de rendez-vous.
fn attendre_identifiant() -> Option<String> {
    std::thread::sleep(ATTENTE_INITIALE);
    let debut = Instant::now();
    while debut.elapsed() < ATTENTE_MAX {
        if Config::get_key_confirmed() {
            let id = Config::get_id();
            if !id.is_empty() {
                return Some(id);
            }
        }
        std::thread::sleep(PAS);
    }
    None
}

fn annoncer(id: &str, jeton: Option<&str>) -> ResultType<()> {
    let corps = serde_json::json!({
        "id": id,
        "machine": crate::common::hostname(),
        "jeton": jeton,
    });
    let client = crate::hbbs_http::create_http_client_with_url_strict(URL_ANNONCE)?;
    let reponse = client
        .post(URL_ANNONCE)
        .timeout(DELAI_REQUETE)
        .json(&corps)
        .send()?;
    if !reponse.status().is_success() {
        bail!("reponse {}", reponse.status());
    }
    Ok(())
}

/// A appeler une fois, dans le processus principal de l'interface.
pub fn demarrer() {
    if !est_variante_sos() {
        return;
    }
    std::thread::spawn(|| {
        let jeton = jeton_du_nom(&nom_d_origine());
        let Some(id) = attendre_identifiant() else {
            log::warn!("ORDiDOC SOS : identifiant non confirme, poste non annonce");
            return;
        };
        for essai in 1..=ESSAIS {
            match annoncer(&id, jeton.as_deref()) {
                Ok(()) => {
                    log::info!("ORDiDOC SOS : poste annonce");
                    return;
                }
                Err(e) => {
                    log::warn!("ORDiDOC SOS : annonce impossible (essai {}) : {}", essai, e);
                    std::thread::sleep(Duration::from_secs(10 * essai));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::jeton_du_nom;

    #[test]
    fn jeton_lu_dans_le_nom() {
        let attendu = Some("Ab3xK9mP2qRs".to_owned());
        assert_eq!(jeton_du_nom("SOS-ORDiDOC-Ab3xK9mP2qRs.exe"), attendu);
        assert_eq!(jeton_du_nom("SOS-ORDiDOC-Ab3xK9mP2qRs (1).exe"), attendu);
        assert_eq!(jeton_du_nom("SOS-ORDiDOC-Ab3xK9mP2qRs(2).exe"), attendu);
        assert_eq!(jeton_du_nom("C:\\Users\\a\\Downloads\\SOS-ORDiDOC-Ab3xK9mP2qRs.exe"), attendu);
    }

    #[test]
    fn pas_de_jeton_dans_les_noms_courants() {
        assert_eq!(jeton_du_nom("SOS-ORDiDOC.exe"), None);
        assert_eq!(jeton_du_nom("SOS-ORDiDOC (1).exe"), None);
        assert_eq!(jeton_du_nom("sos-ordidoc-1.5.3-x86_64.exe"), None);
        assert_eq!(jeton_du_nom("rustdesk.exe"), None);
        assert_eq!(jeton_du_nom(""), None);
    }
}
