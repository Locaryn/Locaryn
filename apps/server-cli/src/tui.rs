//! L'interface plein écran de `locaryn-server`.
//!
//! Un en-tête qui dit l'essentiel (le service tourne-t-il, à quelle adresse,
//! l'API est-elle ouverte), cinq onglets, et une ligne de commande : `/`
//! accepte les mêmes commandes que `locaryn-server <commande>`. Chaque action
//! a sa touche, rappelée en bas de l'écran.

use crate::serveur::{self as s, EtatService};
use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use locaryn_storage::users::{Role, User};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap,
};
use ratatui::{DefaultTerminal, Frame};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

const ACCENT: Color = Color::Rgb(110, 190, 145);
const DOUX: Color = Color::Rgb(130, 130, 140);
const ALERTE: Color = Color::Rgb(230, 170, 90);
const ERREUR: Color = Color::Rgb(230, 100, 100);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Onglet {
    Apercu,
    Comptes,
    Api,
    Modeles,
    Journal,
}

const ONGLETS: [(Onglet, &str); 5] = [
    (Onglet::Apercu, "Aperçu"),
    (Onglet::Comptes, "Comptes"),
    (Onglet::Api, "API & clés"),
    (Onglet::Modeles, "Modèles"),
    (Onglet::Journal, "Journal"),
];

#[derive(Default)]
struct Donnees {
    service: Option<EtatService>,
    port: u16,
    adresse: String,
    comptes: Vec<User>,
    api: bool,
    api_locale: String,
    api_reseau: String,
    cles: Vec<s::Cle>,
    modeles: Vec<s::Modele>,
    journal: Vec<String>,
}

async fn lire() -> Donnees {
    let a = s::adresses_api();
    Donnees {
        service: Some(s::etat_service()),
        port: s::port(),
        adresse: s::adresse_locale(),
        comptes: s::comptes().await.unwrap_or_default(),
        api: s::api_active(),
        api_locale: a.locale,
        api_reseau: a.reseau,
        cles: s::cles().await.unwrap_or_default(),
        modeles: s::modeles(),
        journal: s::fin_du_journal(400).lines().map(str::to_string).collect(),
    }
}

#[derive(Clone)]
enum Champ {
    Texte {
        libelle: String,
        valeur: String,
        secret: bool,
    },
    Case {
        libelle: String,
        valeur: bool,
    },
    Choix {
        libelle: String,
        options: Vec<(&'static str, Option<i64>)>,
        index: usize,
    },
}

#[derive(Clone)]
enum But {
    CreerCompte,
    MotDePasse(String),
    CreerCle,
}

#[derive(Clone)]
struct Formulaire {
    titre: String,
    champs: Vec<Champ>,
    actif: usize,
    but: But,
    erreur: Option<String>,
}

enum Mode {
    Normal,
    Commande(String),
    Formulaire(Formulaire),
    /// Une question oui / non, et la commande à lancer sur « oui ».
    Confirmer(String, Vec<String>),
    /// Une clé tout juste créée : montrée une fois.
    Cle(String, String),
    Aide,
}

struct App {
    onglet: Onglet,
    d: Donnees,
    selection: [ListState; 5],
    mode: Mode,
    /// Ce que les dernières actions ont rendu (texte, succès).
    activite: Vec<(String, bool)>,
    /// L'action en cours, s'il y en a une (« Démarrage du service… »).
    en_cours: Option<String>,
    quitter: bool,
    lu: Instant,
    defilement_journal: usize,
}

impl App {
    fn index(&self) -> usize {
        ONGLETS
            .iter()
            .position(|(o, _)| *o == self.onglet)
            .unwrap_or(0)
    }
    fn selectionne(&self) -> usize {
        self.selection[self.index()].selected().unwrap_or(0)
    }
    fn longueur(&self) -> usize {
        match self.onglet {
            Onglet::Comptes => self.d.comptes.len(),
            Onglet::Api => self.d.cles.len(),
            Onglet::Modeles => self.d.modeles.len(),
            _ => 0,
        }
    }
    fn dire(&mut self, texte: impl Into<String>, ok: bool) {
        for l in texte.into().lines().filter(|l| !l.trim().is_empty()) {
            self.activite.push((l.to_string(), ok));
        }
        let trop = self.activite.len().saturating_sub(50);
        self.activite.drain(..trop);
    }
}

/// Une commande lancée en arrière-plan : l'écran reste vivant pendant qu'un
/// service démarre (jusqu'à quinze secondes).
fn lancer_commande(
    tx: &mpsc::UnboundedSender<(Result<String, String>, bool)>,
    mots: Vec<String>,
    mot_de_passe: Option<String>,
) {
    let tx = tx.clone();
    tokio::spawn(async move {
        let mut args = vec!["locaryn-server".to_string()];
        args.extend(mots);
        let r = match Ligne::try_parse_from(&args) {
            Ok(Ligne { cmd }) => crate::executer(cmd, false, mot_de_passe)
                .await
                .map_err(|e| format!("{e:#}")),
            Err(e) => Err(e.to_string()),
        };
        let _ = tx.send((r, true));
    });
}

/// La ligne de commande de l'interface : les commandes de `locaryn-server`.
#[derive(Parser)]
#[command(name = "locaryn-server", no_binary_name = false)]
struct Ligne {
    #[command(subcommand)]
    cmd: crate::Cmd,
}

/// Couper une ligne en mots, guillemets compris (`keys create "Cursor du portable"`).
fn mots(ligne: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut courant = String::new();
    let mut guillemet: Option<char> = None;
    let mut a_un_mot = false;
    for c in ligne.chars() {
        match (guillemet, c) {
            (Some(q), c) if c == q => guillemet = None,
            (Some(_), c) => courant.push(c),
            (None, '"' | '\'') => {
                guillemet = Some(c);
                a_un_mot = true;
            }
            (None, c) if c.is_whitespace() => {
                if a_un_mot || !courant.is_empty() {
                    out.push(std::mem::take(&mut courant));
                    a_un_mot = false;
                }
            }
            (None, c) => courant.push(c),
        }
    }
    if a_un_mot || !courant.is_empty() {
        out.push(courant);
    }
    out
}

/// Copier dans le presse-papiers du terminal (OSC 52) : Windows Terminal,
/// iTerm, kitty, WezTerm et la plupart des terminaux récents l'acceptent.
fn copier(texte: &str) {
    use std::io::Write;
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let octets = texte.as_bytes();
    let mut enc = String::new();
    for bloc in octets.chunks(3) {
        let n = (bloc[0] as u32) << 16
            | (*bloc.get(1).unwrap_or(&0) as u32) << 8
            | *bloc.get(2).unwrap_or(&0) as u32;
        enc.push(B64[(n >> 18) as usize & 63] as char);
        enc.push(B64[(n >> 12) as usize & 63] as char);
        enc.push(if bloc.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        enc.push(if bloc.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    let mut sortie = std::io::stdout();
    let _ = write!(sortie, "\x1b]52;c;{enc}\x07");
    let _ = sortie.flush();
}

pub async fn lancer() -> Result<()> {
    let mut terminal = ratatui::init();
    let r = boucle(&mut terminal).await;
    ratatui::restore();
    r
}

async fn boucle(terminal: &mut DefaultTerminal) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<(Result<String, String>, bool)>();
    let mut app = App {
        onglet: Onglet::Apercu,
        d: lire().await,
        selection: Default::default(),
        mode: Mode::Normal,
        activite: Vec::new(),
        en_cours: None,
        quitter: false,
        lu: Instant::now(),
        defilement_journal: 0,
    };
    for l in &mut app.selection {
        l.select(Some(0));
    }
    if app.d.comptes.is_empty() {
        app.dire(
            "Aucun compte : créez un administrateur (onglet Comptes, touche a) avant d'ouvrir le serveur au réseau.",
            false,
        );
    }
    while !app.quitter {
        terminal.draw(|f| dessiner(f, &mut app))?;
        // Les résultats des actions en arrière-plan.
        while let Ok((r, relire)) = rx.try_recv() {
            app.en_cours = None;
            match r {
                Ok(t) if t.starts_with("Clé créée") => {
                    let cle = t
                        .lines()
                        .map(str::trim)
                        .find(|l| l.starts_with("locaryn"))
                        .unwrap_or("")
                        .to_string();
                    copier(&cle);
                    app.mode = Mode::Cle(cle, t.lines().next().unwrap_or("").to_string());
                }
                Ok(t) => app.dire(t, true),
                Err(e) => app.dire(e, false),
            }
            if relire {
                app.d = lire().await;
            }
        }
        if app.lu.elapsed() > Duration::from_secs(2) {
            app.d = lire().await;
            app.lu = Instant::now();
        }
        if event::poll(Duration::from_millis(150))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    touche(&mut app, k, &tx);
                }
            }
        }
    }
    Ok(())
}

fn touche(app: &mut App, k: KeyEvent, tx: &mpsc::UnboundedSender<(Result<String, String>, bool)>) {
    if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
        app.quitter = true;
        return;
    }
    let mode = std::mem::replace(&mut app.mode, Mode::Normal);
    match mode {
        Mode::Normal => touche_normale(app, k, tx),
        Mode::Aide => {}
        Mode::Cle(cle, titre) => {
            if !matches!(k.code, KeyCode::Enter | KeyCode::Esc) {
                app.mode = Mode::Cle(cle, titre);
            } else {
                app.dire(format!("{titre} (copiée dans le presse-papiers)"), true);
            }
        }
        Mode::Confirmer(question, commande) => match k.code {
            KeyCode::Char('o' | 'O' | 'y' | 'Y') | KeyCode::Enter => {
                app.en_cours = Some("En cours…".into());
                lancer_commande(tx, commande, None);
            }
            KeyCode::Esc | KeyCode::Char('n' | 'N') => {}
            _ => app.mode = Mode::Confirmer(question, commande),
        },
        Mode::Commande(mut ligne) => match k.code {
            KeyCode::Esc => {}
            KeyCode::Enter => executer_ligne(app, &ligne, tx),
            KeyCode::Backspace => {
                ligne.pop();
                app.mode = Mode::Commande(ligne);
            }
            KeyCode::Char(c) => {
                ligne.push(c);
                app.mode = Mode::Commande(ligne);
            }
            _ => app.mode = Mode::Commande(ligne),
        },
        Mode::Formulaire(mut f) => {
            if formulaire(app, &mut f, k, tx) {
                app.mode = Mode::Formulaire(f);
            }
        }
    }
}

fn executer_ligne(
    app: &mut App,
    ligne: &str,
    tx: &mpsc::UnboundedSender<(Result<String, String>, bool)>,
) {
    let mut m = mots(ligne.trim().trim_start_matches(['/', ':']));
    if m.first().is_some_and(|x| x == "locaryn-server") {
        m.remove(0);
    }
    match m.first().map(String::as_str) {
        None => {}
        Some("quit" | "exit" | "q") => app.quitter = true,
        Some("help" | "aide" | "?") if m.len() == 1 => app.mode = Mode::Aide,
        Some("tui") => {}
        Some("logs") if m.iter().any(|x| x == "-f" || x == "--follow") => {
            app.onglet = Onglet::Journal;
        }
        // Un mot de passe ne s'écrit pas en clair dans une ligne de commande.
        Some("users") if matches!(m.get(1).map(String::as_str), Some("add" | "passwd")) => {
            let nom = m.get(2).cloned().unwrap_or_default();
            if m[1] == "add" {
                let mut f = formulaire_compte();
                if let Champ::Texte { valeur, .. } = &mut f.champs[0] {
                    *valeur = nom;
                }
                if let Champ::Case { valeur, .. } = &mut f.champs[3] {
                    *valeur = m.iter().any(|x| x == "--admin");
                }
                app.mode = Mode::Formulaire(f);
            } else {
                app.mode = Mode::Formulaire(formulaire_mot_de_passe(nom));
            }
        }
        Some(_) => {
            app.dire(format!("› {}", m.join(" ")), true);
            app.en_cours = Some(format!("{}…", m.join(" ")));
            lancer_commande(tx, m, None);
        }
    }
}

fn formulaire_compte() -> Formulaire {
    Formulaire {
        titre: "Nouveau compte".into(),
        champs: vec![
            Champ::Texte {
                libelle: "Nom".into(),
                valeur: String::new(),
                secret: false,
            },
            Champ::Texte {
                libelle: "Mot de passe (8 caractères min.)".into(),
                valeur: String::new(),
                secret: true,
            },
            Champ::Texte {
                libelle: "Confirmer".into(),
                valeur: String::new(),
                secret: true,
            },
            Champ::Case {
                libelle: "Administrateur (comptes, clés d'API)".into(),
                valeur: false,
            },
        ],
        actif: 0,
        but: But::CreerCompte,
        erreur: None,
    }
}

fn formulaire_mot_de_passe(nom: String) -> Formulaire {
    Formulaire {
        titre: format!("Nouveau mot de passe de « {nom} »"),
        champs: vec![
            Champ::Texte {
                libelle: "Mot de passe".into(),
                valeur: String::new(),
                secret: true,
            },
            Champ::Texte {
                libelle: "Confirmer".into(),
                valeur: String::new(),
                secret: true,
            },
        ],
        actif: 0,
        but: But::MotDePasse(nom),
        erreur: None,
    }
}

fn formulaire_cle() -> Formulaire {
    Formulaire {
        titre: "Nouvelle clé d'API".into(),
        champs: vec![
            Champ::Texte {
                libelle: "Application (ex : Cursor du portable)".into(),
                valeur: String::new(),
                secret: false,
            },
            Champ::Choix {
                libelle: "Validité".into(),
                options: vec![
                    ("90 jours", Some(90)),
                    ("30 jours", Some(30)),
                    ("sans expiration", None),
                ],
                index: 0,
            },
        ],
        actif: 0,
        but: But::CreerCle,
        erreur: None,
    }
}

fn texte_du_champ(f: &Formulaire, i: usize) -> String {
    match &f.champs[i] {
        Champ::Texte { valeur, .. } => valeur.clone(),
        _ => String::new(),
    }
}

/// Une touche dans un formulaire. `false` : il se ferme.
fn formulaire(
    app: &mut App,
    f: &mut Formulaire,
    k: KeyEvent,
    tx: &mpsc::UnboundedSender<(Result<String, String>, bool)>,
) -> bool {
    let n = f.champs.len();
    match k.code {
        KeyCode::Esc => return false,
        KeyCode::Tab | KeyCode::Down => f.actif = (f.actif + 1) % n,
        KeyCode::BackTab | KeyCode::Up => f.actif = (f.actif + n - 1) % n,
        KeyCode::Enter => {
            // Entrée passe au champ suivant, et valide sur le dernier.
            if f.actif + 1 < n {
                f.actif += 1;
                return true;
            }
            match valider(f) {
                Ok((commande, mdp)) => {
                    app.en_cours = Some("En cours…".into());
                    lancer_commande(tx, commande, mdp);
                    return false;
                }
                Err(e) => f.erreur = Some(e),
            }
        }
        code => match &mut f.champs[f.actif] {
            Champ::Texte { valeur, .. } => match code {
                KeyCode::Backspace => {
                    valeur.pop();
                }
                KeyCode::Char(c) => valeur.push(c),
                _ => {}
            },
            Champ::Case { valeur, .. } => {
                if matches!(code, KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right) {
                    *valeur = !*valeur;
                }
            }
            Champ::Choix { options, index, .. } => match code {
                KeyCode::Char(' ') | KeyCode::Right => *index = (*index + 1) % options.len(),
                KeyCode::Left => *index = (*index + options.len() - 1) % options.len(),
                _ => {}
            },
        },
    }
    true
}

/// La commande que donne un formulaire rempli, et le mot de passe à part.
fn valider(f: &Formulaire) -> Result<(Vec<String>, Option<String>), String> {
    match &f.but {
        But::CreerCompte => {
            let nom = texte_du_champ(f, 0).trim().to_string();
            let (a, b) = (texte_du_champ(f, 1), texte_du_champ(f, 2));
            if nom.is_empty() {
                return Err("Donnez un nom.".into());
            }
            if a.chars().count() < 8 {
                return Err("Mot de passe trop court (8 caractères minimum).".into());
            }
            if a != b {
                return Err("Les deux mots de passe diffèrent.".into());
            }
            let admin = matches!(f.champs[3], Champ::Case { valeur: true, .. });
            let mut c = vec!["users".into(), "add".into(), nom];
            if admin {
                c.push("--admin".into());
            }
            Ok((c, Some(a)))
        }
        But::MotDePasse(nom) => {
            let (a, b) = (texte_du_champ(f, 0), texte_du_champ(f, 1));
            if a.chars().count() < 8 {
                return Err("Mot de passe trop court (8 caractères minimum).".into());
            }
            if a != b {
                return Err("Les deux mots de passe diffèrent.".into());
            }
            Ok((vec!["users".into(), "passwd".into(), nom.clone()], Some(a)))
        }
        But::CreerCle => {
            let nom = texte_du_champ(f, 0).trim().to_string();
            if nom.is_empty() {
                return Err("Nommez l'application qui utilisera la clé.".into());
            }
            let mut c = vec!["keys".into(), "create".into(), nom];
            if let Champ::Choix { options, index, .. } = &f.champs[1] {
                if let Some(j) = options[*index].1 {
                    c.push("--days".into());
                    c.push(j.to_string());
                }
            }
            Ok((c, None))
        }
    }
}

fn touche_normale(
    app: &mut App,
    k: KeyEvent,
    tx: &mpsc::UnboundedSender<(Result<String, String>, bool)>,
) {
    let i = app.index();
    let nb = app.longueur();
    let commande = |app: &mut App, c: &[&str], texte: &str| {
        app.en_cours = Some(texte.into());
        lancer_commande(tx, c.iter().map(|x| x.to_string()).collect(), None);
    };
    match k.code {
        KeyCode::Char('q') => app.quitter = true,
        KeyCode::Char('/' | ':') => app.mode = Mode::Commande(String::new()),
        KeyCode::Char('?') => app.mode = Mode::Aide,
        KeyCode::Tab | KeyCode::Right => app.onglet = ONGLETS[(i + 1) % 5].0,
        KeyCode::BackTab | KeyCode::Left => app.onglet = ONGLETS[(i + 4) % 5].0,
        KeyCode::Char(c @ '1'..='5') => app.onglet = ONGLETS[c as usize - '1' as usize].0,
        KeyCode::Up | KeyCode::Char('k') => {
            if app.onglet == Onglet::Journal {
                app.defilement_journal = app.defilement_journal.saturating_add(1);
            } else if nb > 0 {
                let s = app.selectionne();
                app.selection[i].select(Some(s.saturating_sub(1)));
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.onglet == Onglet::Journal {
                app.defilement_journal = app.defilement_journal.saturating_sub(1);
            } else if nb > 0 {
                let s = app.selectionne();
                app.selection[i].select(Some((s + 1).min(nb - 1)));
            }
        }
        KeyCode::Char('s') => match app.d.service {
            Some(EtatService::EnMarche { .. }) => {
                app.mode = Mode::Confirmer(
                    "Arrêter le service ? Les appareils et applications connectés perdront l'accès.".into(),
                    vec!["stop".into()],
                );
            }
            _ => commande(app, &["start"], "Démarrage du service…"),
        },
        KeyCode::Char('r') => commande(app, &["restart"], "Redémarrage du service…"),
        KeyCode::Char('a') if app.onglet == Onglet::Comptes => {
            app.mode = Mode::Formulaire(formulaire_compte())
        }
        KeyCode::Char('n') if app.onglet == Onglet::Api => {
            if app.d.api {
                app.mode = Mode::Formulaire(formulaire_cle());
            } else {
                app.dire("Allumez d'abord l'API fournisseur (Espace).", false);
            }
        }
        KeyCode::Char(' ') | KeyCode::Enter if app.onglet == Onglet::Api => {
            let c = if app.d.api { "off" } else { "on" };
            commande(app, &["api", c], "…");
        }
        _ if app.onglet == Onglet::Comptes && nb > 0 => {
            let u = app.d.comptes[app.selectionne().min(nb - 1)].clone();
            let nom = u.username.clone();
            match k.code {
                KeyCode::Char('d') => {
                    let c = if u.disabled { "enable" } else { "disable" };
                    commande(app, &["users", c, &nom], "…");
                }
                KeyCode::Char('A') => {
                    if u.role == Role::Admin {
                        commande(app, &["users", "admin", &nom, "--remove"], "…");
                    } else {
                        commande(app, &["users", "admin", &nom], "…");
                    }
                }
                KeyCode::Char('p') => app.mode = Mode::Formulaire(formulaire_mot_de_passe(nom)),
                KeyCode::Char('x') | KeyCode::Delete => {
                    app.mode = Mode::Confirmer(
                        format!("Supprimer le compte « {nom} » et ses jetons ?"),
                        vec!["users".into(), "remove".into(), nom],
                    )
                }
                _ => {}
            }
        }
        KeyCode::Char('x') | KeyCode::Delete if app.onglet == Onglet::Api && nb > 0 => {
            let c = &app.d.cles[app.selectionne().min(nb - 1)];
            let nom = c.info.label.clone().unwrap_or_else(|| c.info.hint.clone());
            app.mode = Mode::Confirmer(
                format!("Révoquer la clé « {nom} » ? L'application qui l'utilise perd l'accès."),
                vec!["keys".into(), "revoke".into(), c.info.id.to_string()],
            );
        }
        _ => {}
    }
}

// ── Dessin ─────────────────────────────────────────────────────────────────

fn dessiner(f: &mut Frame, app: &mut App) {
    let zones = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Min(6),
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(f.area());
    entete(f, zones[0], app);
    onglets(f, zones[1], app);
    match app.onglet {
        Onglet::Apercu => apercu(f, zones[2], app),
        Onglet::Comptes => comptes(f, zones[2], app),
        Onglet::Api => api(f, zones[2], app),
        Onglet::Modeles => modeles(f, zones[2], app),
        Onglet::Journal => journal(f, zones[2], app),
    }
    activite(f, zones[3], app);
    ligne_de_commande(f, zones[4], app);
    aide_courte(f, zones[5], app);
    match &app.mode {
        Mode::Formulaire(form) => dessiner_formulaire(f, form),
        Mode::Confirmer(q, _) => fenetre(
            f,
            "Confirmer",
            &format!("{q}\n\nEntrée / o : oui    Échap / n : non"),
            ALERTE,
        ),
        Mode::Cle(cle, titre) => fenetre(
            f,
            "Clé d'API",
            &format!(
                "{titre}\n\n{cle}\n\nCopiez-la maintenant : elle ne sera plus affichée.\n\
                 (Copiée dans le presse-papiers si votre terminal le permet.)\n\nEntrée pour fermer"
            ),
            ACCENT,
        ),
        Mode::Aide => fenetre(f, "Aide", AIDE, ACCENT),
        _ => {}
    }
}

const AIDE: &str = "Navigation\n  Tab / ← →  ou 1-5   changer d'onglet\n  ↑ ↓                  choisir dans la liste\n  /                    ligne de commande (mêmes commandes que locaryn-server --help)\n  ?                    cette aide        q  quitter\n\n\
Service\n  s   démarrer / arrêter        r   redémarrer\n\n\
Comptes\n  a   nouveau compte    p   mot de passe    A   administrateur oui/non\n  d   désactiver / réactiver     x   supprimer\n\n\
API & clés\n  Espace   allumer / éteindre l'API fournisseur\n  n   nouvelle clé       x   révoquer la clé choisie\n\n\
Exemples de commandes\n  /users add marie --admin     /keys create \"Cursor\" --days 90\n  /api status                  /config port 7474\n\n\
Une touche pour fermer";

fn entete(f: &mut Frame, zone: Rect, app: &App) {
    let (pastille, etat) = match app.d.service {
        Some(ref e @ EtatService::EnMarche { pid, .. }) => (
            Span::styled("● ", Style::default().fg(ACCENT)),
            Span::raw(format!(
                "Service en marche   {}{}",
                e.adresse(app.d.port).unwrap_or_default(),
                pid.map(|p| format!("   pid {p}")).unwrap_or_default()
            )),
        ),
        _ => (
            Span::styled("○ ", Style::default().fg(DOUX)),
            Span::styled(
                "Service arrêté — s pour le démarrer",
                Style::default().fg(DOUX),
            ),
        ),
    };
    let admins = app
        .d
        .comptes
        .iter()
        .filter(|u| u.role == Role::Admin)
        .count();
    let resume = Line::from(vec![
        Span::styled(
            format!("Comptes {} ", app.d.comptes.len()),
            Style::default(),
        ),
        Span::styled(format!("({admins} admin)"), Style::default().fg(DOUX)),
        Span::styled("  ·  ", Style::default().fg(DOUX)),
        if app.d.api {
            Span::styled("API fournisseur activée", Style::default().fg(ACCENT))
        } else {
            Span::styled("API fournisseur désactivée", Style::default().fg(DOUX))
        },
        Span::styled("  ·  ", Style::default().fg(DOUX)),
        Span::raw(format!("{} clé(s)", app.d.cles.len())),
        Span::styled("  ·  ", Style::default().fg(DOUX)),
        Span::raw(format!("{} modèle(s)", app.d.modeles.len())),
    ]);
    let bloc = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .padding(Padding::horizontal(1))
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" Locaryn Server v{} ", env!("CARGO_PKG_VERSION")),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    f.render_widget(
        Paragraph::new(vec![Line::from(vec![pastille, etat]), resume]).block(bloc),
        zone,
    );
}

fn onglets(f: &mut Frame, zone: Rect, app: &App) {
    let mut spans = vec![Span::raw(" ")];
    for (i, (o, nom)) in ONGLETS.iter().enumerate() {
        let actif = *o == app.onglet;
        spans.push(Span::styled(
            format!(" {} {nom} ", i + 1),
            if actif {
                Style::default()
                    .fg(Color::Black)
                    .bg(ACCENT)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(DOUX)
            },
        ));
        spans.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), zone);
}

fn cadre(titre: &str) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .padding(Padding::horizontal(1))
        .border_style(Style::default().fg(DOUX))
        .title(Span::styled(
            format!(" {titre} "),
            Style::default().add_modifier(Modifier::BOLD),
        ))
}

fn apercu(f: &mut Frame, zone: Rect, app: &App) {
    let en_marche = matches!(app.d.service, Some(EtatService::EnMarche { .. }));
    let mut l: Vec<Line> = Vec::new();
    let titre = |t: &str| {
        Line::from(Span::styled(
            t.to_string(),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
    };
    l.push(titre("Pour les appareils (application mobile, client web)"));
    l.push(Line::from(format!(
        "  https://{}:{}",
        app.d.adresse, app.d.port
    )));
    l.push(Line::from(""));
    l.push(titre("Pour les autres applications (API fournisseur)"));
    if app.d.api {
        l.push(Line::from(format!(
            "  Sur cette machine   {}",
            app.d.api_locale
        )));
        l.push(Line::from(format!(
            "  Depuis le réseau    {}",
            app.d.api_reseau
        )));
    } else {
        l.push(Line::from(Span::styled(
            "  Désactivée — onglet API & clés, Espace pour l'allumer",
            Style::default().fg(DOUX),
        )));
    }
    l.push(Line::from(""));
    l.push(titre("À faire"));
    let mut a_faire = Vec::new();
    if app.d.comptes.is_empty() {
        a_faire.push("Créer un compte administrateur (onglet Comptes, a)");
    }
    if !en_marche {
        a_faire.push("Démarrer le service (s)");
    }
    if app.d.api && app.d.cles.is_empty() {
        a_faire.push("Créer une clé d'API pour chaque application (onglet API & clés, n)");
    }
    if a_faire.is_empty() {
        l.push(Line::from(Span::styled(
            "  Rien : le serveur est prêt.",
            Style::default().fg(ACCENT),
        )));
    }
    for t in a_faire {
        l.push(Line::from(format!("  • {t}")));
    }
    f.render_widget(
        Paragraph::new(l)
            .block(cadre("Aperçu"))
            .wrap(Wrap { trim: false }),
        zone,
    );
}

fn liste<'a>(
    f: &mut Frame,
    zone: Rect,
    titre: &'a str,
    items: Vec<ListItem<'a>>,
    vide: &'a str,
    etat: &mut ListState,
) {
    if items.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(vide, Style::default().fg(DOUX))).block(cadre(titre)),
            zone,
        );
        return;
    }
    let l = List::new(items)
        .block(cadre(titre))
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(45, 60, 52))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    f.render_stateful_widget(l, zone, etat);
}

fn comptes(f: &mut Frame, zone: Rect, app: &mut App) {
    let items: Vec<ListItem> = app
        .d
        .comptes
        .iter()
        .map(|u| {
            ListItem::new(Line::from(vec![
                Span::raw(format!("{:<24}", u.username)),
                if u.role == Role::Admin {
                    Span::styled(
                        format!("{:<16}", "administrateur"),
                        Style::default().fg(ACCENT),
                    )
                } else {
                    Span::raw(format!("{:<16}", "membre"))
                },
                if u.disabled {
                    Span::styled("désactivé", Style::default().fg(ERREUR))
                } else {
                    Span::styled("actif", Style::default().fg(DOUX))
                },
            ]))
        })
        .collect();
    let i = app.index();
    liste(
        f,
        zone,
        "Comptes",
        items,
        "Aucun compte — a pour en créer un (le premier est administrateur).",
        &mut app.selection[i],
    );
}

fn api(f: &mut Frame, zone: Rect, app: &mut App) {
    let [haut, bas] = Layout::vertical([Constraint::Length(6), Constraint::Min(3)]).areas(zone);
    let mut l = vec![Line::from(vec![
        Span::raw("API fournisseur  "),
        if app.d.api {
            Span::styled(
                "● activée",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled("○ désactivée", Style::default().fg(DOUX))
        },
        Span::styled("   (Espace pour basculer)", Style::default().fg(DOUX)),
    ])];
    l.push(Line::from(format!(
        "Sur cette machine   {}",
        app.d.api_locale
    )));
    l.push(Line::from(format!(
        "Depuis le réseau    {}",
        app.d.api_reseau
    )));
    l.push(Line::from(Span::styled(
        format!(
            "Claude Code : ANTHROPIC_BASE_URL={}  ANTHROPIC_AUTH_TOKEN=<clé>",
            app.d.api_locale.trim_end_matches("/v1")
        ),
        Style::default().fg(DOUX),
    )));
    f.render_widget(Paragraph::new(l).block(cadre("API fournisseur")), haut);
    let items: Vec<ListItem> = app
        .d
        .cles
        .iter()
        .map(|c| {
            ListItem::new(Line::from(vec![
                Span::raw(format!(
                    "{:<28}",
                    c.info.label.as_deref().unwrap_or("(sans nom)")
                )),
                Span::styled(format!("{:<14}", c.proprietaire), Style::default().fg(DOUX)),
                Span::raw(format!(
                    "utilisée {:<11} expire {}",
                    s::jour(c.info.last_used_at.as_deref()),
                    s::jour(c.info.expires_at.as_deref())
                )),
            ]))
        })
        .collect();
    let i = app.index();
    let vide = if app.d.api {
        "Aucune clé — n pour en créer une par application."
    } else {
        "Allumez l'API fournisseur pour créer des clés."
    };
    liste(f, bas, "Clés d'API", items, vide, &mut app.selection[i]);
}

fn modeles(f: &mut Frame, zone: Rect, app: &mut App) {
    let items: Vec<ListItem> = app
        .d
        .modeles
        .iter()
        .map(|m| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:>9}  ", s::go(m.octets)),
                    Style::default().fg(DOUX),
                ),
                Span::raw(m.nom.clone()),
            ]))
        })
        .collect();
    let i = app.index();
    liste(
        f,
        zone,
        "Modèles installés",
        items,
        "Aucun modèle installé.",
        &mut app.selection[i],
    );
}

fn journal(f: &mut Frame, zone: Rect, app: &App) {
    let hauteur = zone.height.saturating_sub(2) as usize;
    let n = app.d.journal.len();
    let fin = n.saturating_sub(app.defilement_journal.min(n));
    let debut = fin.saturating_sub(hauteur);
    let lignes: Vec<Line> = app.d.journal[debut..fin]
        .iter()
        .map(|l| {
            let style = if l.contains("ERROR") {
                Style::default().fg(ERREUR)
            } else if l.contains("WARN") {
                Style::default().fg(ALERTE)
            } else {
                Style::default()
            };
            Line::from(Span::styled(l.clone(), style))
        })
        .collect();
    f.render_widget(
        Paragraph::new(lignes).block(cadre(if app.defilement_journal == 0 {
            "Journal (suivi en direct)"
        } else {
            "Journal (↓ pour revenir au direct)"
        })),
        zone,
    );
}

fn activite(f: &mut Frame, zone: Rect, app: &App) {
    let n = zone.height as usize;
    let mut lignes: Vec<Line> = app
        .activite
        .iter()
        .rev()
        .take(if app.en_cours.is_some() { n - 1 } else { n })
        .rev()
        .map(|(t, ok)| {
            Line::from(Span::styled(
                format!(" {t}"),
                Style::default().fg(if *ok { Color::Reset } else { ERREUR }),
            ))
        })
        .collect();
    if let Some(e) = &app.en_cours {
        lignes.push(Line::from(Span::styled(
            format!(" ⋯ {e}"),
            Style::default().fg(ALERTE),
        )));
    }
    f.render_widget(Paragraph::new(lignes), zone);
}

fn ligne_de_commande(f: &mut Frame, zone: Rect, app: &App) {
    let l = match &app.mode {
        Mode::Commande(t) => Line::from(vec![
            Span::styled(
                " › /",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::raw(t.clone()),
            Span::styled("▏", Style::default().fg(ACCENT)),
        ]),
        _ => Line::from(Span::styled(
            " › / pour une commande  ·  ? pour l'aide",
            Style::default().fg(DOUX),
        )),
    };
    f.render_widget(Paragraph::new(l), zone);
}

fn aide_courte(f: &mut Frame, zone: Rect, app: &App) {
    let t = match (&app.mode, app.onglet) {
        (Mode::Commande(_), _) => "Entrée exécuter · Échap annuler · ex. users add marie --admin, keys create \"Cursor\", api status",
        (Mode::Formulaire(_), _) => "Tab/↑↓ champ · Espace case ou choix · Entrée suivant / valider · Échap annuler",
        (_, Onglet::Comptes) => "a nouveau · p mot de passe · A admin · d désactiver · x supprimer · s service · q quitter",
        (_, Onglet::Api) => "Espace API on/off · n nouvelle clé · x révoquer · s service · q quitter",
        (_, Onglet::Journal) => "↑↓ défiler · s service · r redémarrer · q quitter",
        _ => "Tab onglets · s démarrer/arrêter · r redémarrer · / commande · ? aide · q quitter",
    };
    f.render_widget(
        Paragraph::new(Span::styled(format!(" {t}"), Style::default().fg(DOUX))),
        zone,
    );
}

fn centre(zone: Rect, largeur: u16, hauteur: u16) -> Rect {
    let l = largeur.min(zone.width.saturating_sub(4));
    let h = hauteur.min(zone.height.saturating_sub(2));
    Rect::new(
        zone.x + (zone.width - l) / 2,
        zone.y + (zone.height - h) / 2,
        l,
        h,
    )
}

fn fenetre(f: &mut Frame, titre: &str, texte: &str, couleur: Color) {
    let hauteur = texte.lines().count() as u16 + 4;
    let zone = centre(f.area(), 84, hauteur);
    f.render_widget(Clear, zone);
    f.render_widget(
        Paragraph::new(Text::from(texte.to_string()))
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .padding(Padding::horizontal(1))
                    .border_style(Style::default().fg(couleur))
                    .title(Span::styled(
                        format!(" {titre} "),
                        Style::default().fg(couleur).add_modifier(Modifier::BOLD),
                    )),
            ),
        zone,
    );
}

fn dessiner_formulaire(f: &mut Frame, form: &Formulaire) {
    let mut l: Vec<Line> = Vec::new();
    for (i, c) in form.champs.iter().enumerate() {
        let actif = i == form.actif;
        let marque = if actif { "› " } else { "  " };
        let style = if actif {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let (libelle, valeur) = match c {
            Champ::Texte {
                libelle,
                valeur,
                secret,
            } => (
                libelle.clone(),
                if *secret {
                    "•".repeat(valeur.chars().count())
                } else {
                    valeur.clone()
                } + if actif { "▏" } else { "" },
            ),
            Champ::Case { libelle, valeur } => (
                libelle.clone(),
                if *valeur {
                    "[x] oui".into()
                } else {
                    "[ ] non".into()
                },
            ),
            Champ::Choix {
                libelle,
                options,
                index,
            } => (libelle.clone(), format!("‹ {} ›", options[*index].0)),
        };
        l.push(Line::from(vec![Span::styled(
            format!("{marque}{libelle}"),
            style,
        )]));
        l.push(Line::from(format!("    {valeur}")));
    }
    if let Some(e) = &form.erreur {
        l.push(Line::from(""));
        l.push(Line::from(Span::styled(
            e.clone(),
            Style::default().fg(ERREUR),
        )));
    }
    l.push(Line::from(""));
    l.push(Line::from(Span::styled(
        "Entrée : suivant / valider    Échap : annuler",
        Style::default().fg(DOUX),
    )));
    let zone = centre(f.area(), 64, l.len() as u16 + 2);
    f.render_widget(Clear, zone);
    f.render_widget(
        Paragraph::new(l).alignment(Alignment::Left).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .padding(Padding::horizontal(1))
                .border_style(Style::default().fg(ACCENT))
                .title(Span::styled(
                    format!(" {} ", form.titre),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                )),
        ),
        zone,
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn une_ligne_se_coupe_en_mots_guillemets_compris() {
        assert_eq!(
            super::mots(r#"keys create "Cursor du portable" --days 30"#),
            ["keys", "create", "Cursor du portable", "--days", "30"]
        );
        assert_eq!(super::mots("  users   list "), ["users", "list"]);
        assert_eq!(super::mots(r#"keys create """#), ["keys", "create", ""]);
    }

    #[test]
    fn la_ligne_de_commande_comprend_les_commandes_du_programme() {
        use clap::Parser as _;
        let l =
            super::Ligne::try_parse_from(["locaryn-server", "users", "add", "marie", "--admin"])
                .unwrap();
        assert!(matches!(
            l.cmd,
            crate::Cmd::Users {
                action: crate::UsersCmd::Add { admin: true, .. }
            }
        ));
        assert!(super::Ligne::try_parse_from(["locaryn-server", "inconnue"]).is_err());
    }
}

#[cfg(test)]
mod rendu {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn app() -> App {
        let mut a = App {
            onglet: Onglet::Apercu,
            d: Donnees {
                service: Some(EtatService::EnMarche {
                    pid: Some(14492),
                    reseau: true,
                }),
                port: 7474,
                adresse: "192.168.0.57".into(),
                comptes: vec![
                    User {
                        id: uuid::Uuid::new_v4(),
                        username: "tealo".into(),
                        role: Role::Admin,
                        disabled: false,
                    },
                    User {
                        id: uuid::Uuid::new_v4(),
                        username: "marie".into(),
                        role: Role::Member,
                        disabled: true,
                    },
                ],
                api: true,
                api_locale: "http://127.0.0.1:7475/v1".into(),
                api_reseau: "https://192.168.0.57:7474/v1".into(),
                cles: vec![],
                modeles: vec![s::Modele {
                    nom: "unsloth__Qwen3.5-9B-GGUF/Qwen3.5-9B-UD-IQ3_XXS.gguf".into(),
                    octets: 3_900_000_000,
                }],
                journal: vec![
                    "2026-10-16 INFO locaryn-daemon à l'écoute".into(),
                    "2026-10-16 WARN exemple".into(),
                ],
            },
            selection: Default::default(),
            mode: Mode::Normal,
            activite: vec![("Service démarré (pid 14492)".into(), true)],
            en_cours: None,
            quitter: false,
            lu: Instant::now(),
            defilement_journal: 0,
        };
        for l in &mut a.selection {
            l.select(Some(0));
        }
        a
    }

    fn ecran(app: &mut App) -> String {
        let mut t = Terminal::new(TestBackend::new(100, 30)).unwrap();
        t.draw(|f| dessiner(f, app)).unwrap();
        let b = t.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..b.area.height {
            for x in 0..b.area.width {
                out.push_str(b[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    #[ignore]
    fn montrer() {
        let mut a = app();
        for o in [
            Onglet::Apercu,
            Onglet::Comptes,
            Onglet::Api,
            Onglet::Journal,
        ] {
            a.onglet = o;
            println!("{}", ecran(&mut a));
        }
        a.onglet = Onglet::Comptes;
        a.mode = Mode::Formulaire(formulaire_compte());
        println!("{}", ecran(&mut a));
    }
}
