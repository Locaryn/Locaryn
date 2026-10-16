//! L'interface de `locaryn` dans un terminal : l'agent, à la manière de
//! Claude Code.
//!
//! La conversation s'écrit dans le terminal lui-même (elle reste dans son
//! historique, on la fait défiler comme d'habitude) ; seul le bas est
//! redessiné : la ligne d'activité (réflexion, fichier en cours d'écriture),
//! le cadre de saisie et la ligne d'état — modèle, contexte occupé, vitesse.
//! Les commandes commencent par `/` (`/help` les liste).
//!
//! `locaryn --plain`, ou un terminal qui n'en est pas un, garde la boucle
//! texte d'avant.

use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use locaryn_agent_runtime::reasoning::{peek, split_reasoning};
use locaryn_events::{LogLevel, StreamEvent};
use locaryn_sdk::LocarynClient;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Widget};
use ratatui::{Terminal, TerminalOptions, Viewport};
use unicode_width::UnicodeWidthChar;

const ACCENT: Color = Color::Rgb(110, 190, 145);
const DOUX: Color = Color::Rgb(135, 135, 145);
const ALERTE: Color = Color::Rgb(230, 170, 90);
const ERREUR: Color = Color::Rgb(230, 100, 100);
const HAUTEUR: u16 = 7;
const TOUPIE: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

type Term = Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>;
type Flux = std::pin::Pin<
    Box<dyn futures::Stream<Item = Result<StreamEvent, locaryn_sdk::SdkError>> + Send>,
>;

/// Une demande d'accord en attente.
struct Accord {
    call_id: String,
    outil: String,
    raison: String,
    resume: String,
}

struct Etat {
    agentic: bool,
    modele: String,
    dossier: String,
    session: String,
    saisie: String,
    historique: Vec<String>,
    position: Option<usize>,
    /// La réponse en cours : tout le texte reçu, et ce qui est déjà écrit.
    reponse: String,
    ecrit: usize,
    en_cours: bool,
    voir_reflexion: bool,
    activite: String,
    accord: Option<Accord>,
    toupie: usize,
    contexte: Option<(u64, u64)>,
    detail_contexte: Vec<(String, u64)>,
    vitesse: Option<f32>,
    quitter: bool,
}

/// Couper un texte à `largeur` colonnes, mots entiers quand c'est possible.
pub fn envelopper(texte: &str, largeur: usize) -> Vec<String> {
    let largeur = largeur.max(10);
    let mut out = Vec::new();
    for ligne in texte.split('\n') {
        let mut courante = String::new();
        let mut cols = 0usize;
        for mot in ligne.split_inclusive(' ') {
            let w: usize = mot.chars().map(|c| c.width().unwrap_or(0)).sum();
            if cols + w > largeur && cols > 0 {
                out.push(courante.trim_end().to_string());
                courante.clear();
                cols = 0;
            }
            if w > largeur {
                // Un mot plus long que la ligne : coupé à la colonne.
                for c in mot.chars() {
                    let cw = c.width().unwrap_or(0);
                    if cols + cw > largeur {
                        out.push(std::mem::take(&mut courante));
                        cols = 0;
                    }
                    courante.push(c);
                    cols += cw;
                }
            } else {
                courante.push_str(mot);
                cols += w;
            }
        }
        out.push(courante.trim_end().to_string());
    }
    out
}

/// Écrire des lignes au-dessus de la zone redessinée : elles rejoignent
/// l'historique du terminal.
fn ecrire(term: &mut Term, lignes: Vec<Line<'static>>) {
    if lignes.is_empty() {
        return;
    }
    let h = lignes.len() as u16;
    let _ = term.insert_before(h, |buf| {
        Paragraph::new(lignes).render(buf.area, buf);
    });
}

fn largeur(term: &Term) -> usize {
    term.size().map(|s| s.width as usize).unwrap_or(80)
}

/// Un bloc de texte, préfixé sur sa première ligne (`● `, `⎿ `) et décalé
/// ensuite, enveloppé à la largeur du terminal.
fn bloc(
    term: &Term,
    prefixe: &str,
    style_prefixe: Style,
    texte: &str,
    style: Style,
) -> Vec<Line<'static>> {
    let decalage = prefixe.chars().count();
    let l = largeur(term).saturating_sub(decalage + 1);
    envelopper(texte, l)
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            let p = if i == 0 {
                Span::styled(prefixe.to_string(), style_prefixe)
            } else {
                Span::raw(" ".repeat(decalage))
            };
            Line::from(vec![p, Span::styled(t, style)])
        })
        .collect()
}

fn accueil(term: &mut Term, e: &Etat, version: &str) {
    let l = largeur(term).clamp(40, 100) as u16;
    let lignes = [
        Line::from(vec![
            Span::styled("✻ ", Style::default().fg(ACCENT)),
            Span::styled(
                "Bienvenue dans Locaryn",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  v{version}"), Style::default().fg(DOUX)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Modèle   ", Style::default().fg(DOUX)),
            Span::raw(e.modele.clone()),
        ]),
        Line::from(vec![
            Span::styled("  Dossier  ", Style::default().fg(DOUX)),
            Span::raw(e.dossier.clone()),
        ]),
        Line::from(vec![
            Span::styled("  Mode     ", Style::default().fg(DOUX)),
            Span::raw(if e.agentic {
                "agent — lit, écrit et lance des commandes dans ce dossier"
            } else {
                "conversation simple — aucun accès à vos fichiers"
            }),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "  /help pour les commandes · Ctrl+C pour interrompre · /exit pour quitter",
            Style::default().fg(DOUX),
        )),
    ];
    let h = lignes.len() as u16 + 2;
    let _ = term.insert_before(h, |buf| {
        let zone = Rect::new(buf.area.x, buf.area.y, l.min(buf.area.width), h);
        Paragraph::new(lignes.to_vec())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(ACCENT)),
            )
            .render(zone, buf);
    });
    ecrire(term, vec![Line::from("")]);
}

const AIDE: &[(&str, &str)] = &[
    ("/help", "cette aide"),
    ("/status", "service, modèle et conversation"),
    ("/context", "ce qui occupe la fenêtre de contexte"),
    ("/think", "afficher ou masquer le raisonnement du modèle"),
    ("/new", "nouvelle conversation dans ce dossier"),
    ("/clear", "effacer l'écran"),
    ("/exit", "quitter (aussi Ctrl+D)"),
];

fn ko(n: u64) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

fn dessiner(f: &mut ratatui::Frame, e: &Etat) {
    let zones = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(f.area());

    if let Some(a) = &e.accord {
        // La demande d'accord prend toute la zone.
        let lignes = vec![
            Line::from(vec![
                Span::styled(
                    "? ",
                    Style::default().fg(ALERTE).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{} demande votre accord", a.outil),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(Span::styled(
                format!("  {}", a.raison),
                Style::default().fg(DOUX),
            )),
            Line::from(Span::raw(format!("  {}", a.resume))),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "  o",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" une fois   "),
                Span::styled(
                    "s",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" cette conversation   "),
                Span::styled(
                    "t",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" toujours   "),
                Span::styled(
                    "n",
                    Style::default().fg(ERREUR).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" refuser"),
            ]),
        ];
        f.render_widget(
            Paragraph::new(lignes).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(ALERTE)),
            ),
            f.area(),
        );
        return;
    }

    // Ligne d'activité.
    let activite = if e.en_cours {
        Line::from(vec![
            Span::styled(
                format!("{} ", TOUPIE[e.toupie % TOUPIE.len()]),
                Style::default().fg(ACCENT),
            ),
            Span::styled(
                if e.activite.is_empty() {
                    "Travail en cours…".to_string()
                } else {
                    e.activite.clone()
                },
                Style::default().fg(DOUX),
            ),
            Span::styled("   (Ctrl+C pour interrompre)", Style::default().fg(DOUX)),
        ])
    } else {
        Line::from("")
    };
    f.render_widget(Paragraph::new(activite), zones[0]);

    // Cadre de saisie.
    let place = zones[1].width.saturating_sub(6) as usize;
    let visible: String = {
        let n = e.saisie.chars().count();
        e.saisie.chars().skip(n.saturating_sub(place)).collect()
    };
    let saisie = if e.saisie.is_empty() && !e.en_cours {
        Line::from(vec![
            Span::styled("> ", Style::default().fg(ACCENT)),
            Span::styled("Écrivez votre demande, ou /help", Style::default().fg(DOUX)),
        ])
    } else {
        Line::from(vec![
            Span::styled("> ", Style::default().fg(ACCENT)),
            Span::raw(visible),
            Span::styled("▏", Style::default().fg(ACCENT)),
        ])
    };
    f.render_widget(
        Paragraph::new(saisie).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(if e.en_cours { DOUX } else { ACCENT })),
        ),
        zones[1],
    );

    // Ligne d'état.
    let mut etat = vec![Span::styled(
        format!("  {}", e.modele),
        Style::default().fg(DOUX),
    )];
    if let Some((utilise, fenetre)) = e.contexte {
        let part = if fenetre > 0 {
            utilise as f64 / fenetre as f64
        } else {
            0.0
        };
        etat.push(Span::styled("  ·  contexte ", Style::default().fg(DOUX)));
        etat.push(Span::styled(
            format!("{} / {}", ko(utilise), ko(fenetre)),
            Style::default().fg(if part > 0.85 { ALERTE } else { DOUX }),
        ));
    }
    if let Some(v) = e.vitesse {
        etat.push(Span::styled(
            format!("  ·  {v:.1} t/s"),
            Style::default().fg(DOUX),
        ));
    }
    etat.push(Span::styled(
        if e.agentic {
            "  ·  agent"
        } else {
            "  ·  conversation"
        },
        Style::default().fg(DOUX),
    ));
    if e.voir_reflexion {
        etat.push(Span::styled(
            "  ·  raisonnement affiché",
            Style::default().fg(DOUX),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(etat)), zones[2]);
}

/// Ce qu'on montre d'un appel d'outil : son nom et son argument principal.
fn resume_outil(outil: &str, args: &serde_json::Value) -> String {
    let principal = ["path", "command", "query", "url", "pattern", "name"]
        .iter()
        .find_map(|k| args.get(*k).and_then(|v| v.as_str()));
    match principal {
        Some(p) => {
            let p: String = p.chars().take(70).collect();
            format!("{outil}({p})")
        }
        None => outil.to_string(),
    }
}

/// Écrire ce qui est complet de la réponse ; garder la ligne entamée.
fn vider_reponse(term: &mut Term, e: &mut Etat, tout: bool) {
    let split = split_reasoning(&e.reponse);
    if split.in_progress && !e.voir_reflexion {
        e.activite = format!("Réflexion… {}", peek(&split.reasoning, 60));
        return;
    }
    let texte = &split.answer;
    let fin = if tout {
        texte.len()
    } else {
        texte[e.ecrit.min(texte.len())..]
            .rfind('\n')
            .map(|i| e.ecrit + i + 1)
            .unwrap_or(e.ecrit)
    };
    if fin <= e.ecrit {
        // La ligne entamée s'affiche dans la ligne d'activité.
        let entame: String = texte[e.ecrit.min(texte.len())..].chars().take(90).collect();
        if !entame.trim().is_empty() {
            e.activite = entame.trim().to_string();
        }
        return;
    }
    let morceau = texte[e.ecrit..fin].trim_end_matches('\n').to_string();
    let premier = e.ecrit == 0 || texte[..e.ecrit].trim().is_empty();
    e.ecrit = fin;
    if morceau.trim().is_empty() && !premier {
        ecrire(term, vec![Line::from("")]);
        return;
    }
    let lignes = if premier {
        bloc(
            term,
            "● ",
            Style::default().fg(ACCENT),
            morceau.trim_start(),
            Style::default(),
        )
    } else {
        bloc(term, "  ", Style::default(), &morceau, Style::default())
    };
    ecrire(term, lignes);
}

fn evenement(term: &mut Term, e: &mut Etat, ev: StreamEvent) {
    match ev {
        StreamEvent::Token { text } => {
            e.reponse.push_str(&text);
            vider_reponse(term, e, false);
        }
        StreamEvent::ToolDraft {
            tool, path, chars, ..
        } => {
            e.activite = format!(
                "{} {} ({} car.)",
                if tool == "write_file" {
                    "Écrit"
                } else {
                    "Prépare"
                },
                path.unwrap_or(tool),
                chars
            );
        }
        StreamEvent::ToolCall { tool, args, .. } => {
            vider_reponse(term, e, true);
            // Le texte qui suit l'outil est une nouvelle réponse.
            e.reponse.clear();
            e.ecrit = 0;
            let lignes = bloc(
                term,
                "⏺ ",
                Style::default().fg(ACCENT),
                &resume_outil(&tool, &args),
                Style::default().add_modifier(Modifier::BOLD),
            );
            ecrire(term, lignes);
            e.activite = format!("{tool}…");
        }
        StreamEvent::ToolResult { ok, output, .. } => {
            let premiere = output
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty() && !l.starts_with("[exit 0]"))
                .unwrap_or(if ok { "fait" } else { "échec" });
            let premiere: String = premiere.chars().take(160).collect();
            let lignes = bloc(
                term,
                "  ⎿ ",
                Style::default().fg(DOUX),
                &premiere,
                Style::default().fg(if ok { DOUX } else { ERREUR }),
            );
            ecrire(term, lignes);
            e.activite.clear();
        }
        StreamEvent::Log {
            level: LogLevel::Warn | LogLevel::Error,
            msg,
            ..
        } => {
            let lignes = bloc(
                term,
                "  ! ",
                Style::default().fg(ALERTE),
                &msg,
                Style::default().fg(ALERTE),
            );
            ecrire(term, lignes);
        }
        StreamEvent::Timings {
            generation_tokens_per_sec,
            ..
        } if generation_tokens_per_sec > 0.0 => {
            e.vitesse = Some(generation_tokens_per_sec);
        }
        StreamEvent::ContextUsage { window, parts, .. } => {
            let utilise = parts.iter().map(|p| p.tokens).sum();
            e.contexte = Some((utilise, window));
            e.detail_contexte = parts.into_iter().map(|p| (p.key, p.tokens)).collect();
        }
        StreamEvent::ToolApproval {
            call_id,
            tool,
            args,
            reason,
            ..
        } => {
            vider_reponse(term, e, true);
            let resume: String = resume_outil(&tool, &args).chars().take(110).collect();
            e.accord = Some(Accord {
                call_id,
                outil: tool,
                raison: reason.lines().next().unwrap_or("").to_string(),
                resume,
            });
        }
        StreamEvent::MessageEnd { .. } => {
            vider_reponse(term, e, true);
            if e.voir_reflexion {
                let split = split_reasoning(&e.reponse);
                if !split.reasoning.is_empty() {
                    let l = bloc(
                        term,
                        "  ✻ ",
                        Style::default().fg(DOUX),
                        &split.reasoning,
                        Style::default().fg(DOUX),
                    );
                    ecrire(term, l);
                }
            }
            ecrire(term, vec![Line::from("")]);
            e.en_cours = false;
            e.activite.clear();
        }
        _ => {}
    }
}

/// Une commande `/…` : rend `true` si elle a été traitée ici.
async fn commande(
    term: &mut Term,
    e: &mut Etat,
    client: &LocarynClient,
    ligne: &str,
) -> anyhow::Result<bool> {
    let mot = ligne.split_whitespace().next().unwrap_or("");
    match mot {
        "/exit" | "/quit" | "/q" => e.quitter = true,
        "/help" | "/aide" | "/?" => {
            let mut l = vec![Line::from(Span::styled(
                "Commandes",
                Style::default().add_modifier(Modifier::BOLD),
            ))];
            for (c, d) in AIDE {
                l.push(Line::from(vec![
                    Span::styled(format!("  {c:<10}"), Style::default().fg(ACCENT)),
                    Span::styled(d.to_string(), Style::default().fg(DOUX)),
                ]));
            }
            l.push(Line::from(""));
            ecrire(term, l);
        }
        "/think" => {
            e.voir_reflexion = !e.voir_reflexion;
            ecrire(
                term,
                vec![Line::from(Span::styled(
                    format!(
                        "  raisonnement {}",
                        if e.voir_reflexion {
                            "affiché"
                        } else {
                            "masqué"
                        }
                    ),
                    Style::default().fg(DOUX),
                ))],
            );
        }
        "/status" => {
            let sante = client.health().await.ok();
            let l = vec![
                Line::from(Span::styled(
                    "État",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::from(format!("  service       {}", client.base_url())),
                Line::from(format!(
                    "  version       {}",
                    sante
                        .as_ref()
                        .map(|h| h.version.clone())
                        .unwrap_or_else(|| "injoignable".into())
                )),
                Line::from(format!("  modèle        {}", e.modele)),
                Line::from(format!("  dossier       {}", e.dossier)),
                Line::from(format!("  conversation  {}", e.session)),
                Line::from(""),
            ];
            ecrire(term, l);
        }
        "/context" => {
            let mut l = vec![Line::from(Span::styled(
                "Fenêtre de contexte",
                Style::default().add_modifier(Modifier::BOLD),
            ))];
            match e.contexte {
                None => l.push(Line::from(Span::styled(
                    "  pas encore mesurée — après la première réponse",
                    Style::default().fg(DOUX),
                ))),
                Some((u, w)) => {
                    l.push(Line::from(format!(
                        "  {} / {} jetons ({:.0} %)",
                        ko(u),
                        ko(w),
                        if w > 0 {
                            u as f64 * 100.0 / w as f64
                        } else {
                            0.0
                        }
                    )));
                    for (k, t) in &e.detail_contexte {
                        l.push(Line::from(Span::styled(
                            format!("    {k:<16} {}", ko(*t)),
                            Style::default().fg(DOUX),
                        )));
                    }
                }
            }
            l.push(Line::from(""));
            ecrire(term, l);
        }
        "/new" => {
            e.session = crate::resolve_session(client, None, e.agentic).await?;
            e.contexte = None;
            ecrire(
                term,
                vec![
                    Line::from(Span::styled(
                        "  nouvelle conversation",
                        Style::default().fg(DOUX),
                    )),
                    Line::from(""),
                ],
            );
        }
        "/clear" => {
            let _ = term.clear();
            let _ = crossterm::execute!(
                std::io::stdout(),
                crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
                crossterm::cursor::MoveTo(0, 0)
            );
        }
        m if m.starts_with('/') => {
            ecrire(
                term,
                vec![Line::from(Span::styled(
                    format!("  commande inconnue : {m} — /help pour la liste"),
                    Style::default().fg(ALERTE),
                ))],
            );
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// L'agent dans le terminal. `version` et `modele` viennent du service.
pub async fn converser(
    client: &LocarynClient,
    session: String,
    agentic: bool,
) -> anyhow::Result<()> {
    let sante = client.health().await.ok();
    let modele = sante
        .as_ref()
        .and_then(|h| h.active_provider.as_ref())
        .and_then(|p| p.model.clone())
        .map(|m| {
            m.rsplit('/')
                .next()
                .unwrap_or(&m)
                .trim_end_matches(".gguf")
                .to_string()
        })
        .unwrap_or_else(|| "modèle local".into());
    let version = sante
        .map(|h| h.version)
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").into());
    let dossier = std::env::current_dir()
        .map(|d| d.to_string_lossy().into_owned())
        .unwrap_or_default();

    crossterm::terminal::enable_raw_mode()?;
    let mut term = Terminal::with_options(
        ratatui::backend::CrosstermBackend::new(std::io::stdout()),
        TerminalOptions {
            viewport: Viewport::Inline(HAUTEUR),
        },
    )?;
    let mut e = Etat {
        agentic,
        modele,
        dossier,
        session,
        saisie: String::new(),
        historique: Vec::new(),
        position: None,
        reponse: String::new(),
        ecrit: 0,
        en_cours: false,
        voir_reflexion: false,
        activite: String::new(),
        accord: None,
        toupie: 0,
        contexte: None,
        detail_contexte: Vec::new(),
        vitesse: None,
        quitter: false,
    };
    accueil(&mut term, &e, &version);

    let r = boucle(&mut term, &mut e, client).await;

    // Rendre le terminal tel qu'on l'a trouvé : la zone du bas effacée, le
    // curseur en dessous.
    let _ = term.draw(|f| f.render_widget(Paragraph::new(""), f.area()));
    crossterm::terminal::disable_raw_mode()?;
    println!();
    r
}

async fn boucle(term: &mut Term, e: &mut Etat, client: &LocarynClient) -> anyhow::Result<()> {
    let mut clavier = EventStream::new();
    let mut flux: Option<Flux> = None;
    let mut horloge = tokio::time::interval(std::time::Duration::from_millis(100));
    while !e.quitter {
        term.draw(|f| dessiner(f, e))?;
        tokio::select! {
            _ = horloge.tick() => {
                if e.en_cours {
                    e.toupie += 1;
                }
            }
            ev = async {
                match flux.as_mut() {
                    Some(f) => f.next().await,
                    None => std::future::pending().await,
                }
            } => {
                match ev {
                    Some(Ok(ev)) => evenement(term, e, ev),
                    Some(Err(err)) => {
                        ecrire(term, bloc(term, "  ! ", Style::default().fg(ERREUR), &err.to_string(), Style::default().fg(ERREUR)));
                        flux = None;
                        e.en_cours = false;
                    }
                    None => {
                        flux = None;
                        if e.en_cours {
                            vider_reponse(term, e, true);
                            e.en_cours = false;
                        }
                    }
                }
            }
            touche = clavier.next() => {
                let Some(Ok(Event::Key(k))) = touche else { continue };
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                if let Some(a) = e.accord.take() {
                    let (autorise, portee) = match k.code {
                        KeyCode::Char('o' | 'O' | 'y' | 'Y') | KeyCode::Enter => (true, "once"),
                        KeyCode::Char('s' | 'S') => (true, "session"),
                        KeyCode::Char('t' | 'T') => (true, "always"),
                        KeyCode::Char('n' | 'N') | KeyCode::Esc => (false, "once"),
                        _ => {
                            e.accord = Some(a);
                            continue;
                        }
                    };
                    let texte = match client.answer_approval(&a.call_id, &a.outil, autorise, portee).await {
                        Ok(()) => if autorise { "autorisé" } else { "refusé" }.to_string(),
                        Err(err) => format!("réponse non prise en compte : {err}"),
                    };
                    ecrire(term, vec![Line::from(Span::styled(
                        format!("  ⎿ {} {texte}", a.outil),
                        Style::default().fg(if autorise { DOUX } else { ALERTE }),
                    ))]);
                    continue;
                }
                if let Some(ligne) = clavier_saisie(e, k) {
                    if e.en_cours {
                        continue;
                    }
                    if commande(term, e, client, &ligne).await? {
                        continue;
                    }
                    ecrire(term, bloc(term, "> ", Style::default().fg(DOUX), &ligne, Style::default().fg(DOUX)));
                    ecrire(term, vec![Line::from("")]);
                    e.reponse.clear();
                    e.ecrit = 0;
                    e.en_cours = true;
                    e.activite.clear();
                    match client.send_message(&e.session, &ligne).await {
                        Ok(s) => flux = Some(Box::pin(s)),
                        Err(err) => {
                            e.en_cours = false;
                            ecrire(term, bloc(term, "  ! ", Style::default().fg(ERREUR), &err.to_string(), Style::default().fg(ERREUR)));
                        }
                    }
                } else if e.quitter_demande(k) {
                    if e.en_cours {
                        // Ne plus écouter la réponse : la suivante peut partir.
                        flux = None;
                        vider_reponse(term, e, true);
                        e.en_cours = false;
                        ecrire(term, vec![Line::from(Span::styled("  ⎿ interrompu", Style::default().fg(ALERTE)))]);
                    } else {
                        e.quitter = true;
                    }
                }
            }
        }
    }
    Ok(())
}

impl Etat {
    /// Ctrl+C (ou Ctrl+D sur une saisie vide).
    fn quitter_demande(&self, k: KeyEvent) -> bool {
        k.modifiers.contains(KeyModifiers::CONTROL)
            && (k.code == KeyCode::Char('c')
                || (k.code == KeyCode::Char('d') && self.saisie.is_empty()))
    }
}

/// Une touche dans la saisie ; `Some(ligne)` quand elle est envoyée.
fn clavier_saisie(e: &mut Etat, k: KeyEvent) -> Option<String> {
    if k.modifiers.contains(KeyModifiers::CONTROL) {
        match k.code {
            KeyCode::Char('u') => e.saisie.clear(),
            KeyCode::Char('w') => {
                let t = e.saisie.trim_end();
                let coupe = t.rfind(' ').map(|i| i + 1).unwrap_or(0);
                e.saisie.truncate(coupe);
            }
            _ => {}
        }
        return None;
    }
    match k.code {
        KeyCode::Enter => {
            let ligne = e.saisie.trim().to_string();
            if ligne.is_empty() {
                return None;
            }
            e.saisie.clear();
            e.position = None;
            if e.historique.last() != Some(&ligne) {
                e.historique.push(ligne.clone());
            }
            Some(ligne)
        }
        KeyCode::Backspace => {
            e.saisie.pop();
            None
        }
        KeyCode::Up if !e.historique.is_empty() => {
            let p = e
                .position
                .map(|p| p.saturating_sub(1))
                .unwrap_or(e.historique.len() - 1);
            e.position = Some(p);
            e.saisie = e.historique[p].clone();
            None
        }
        KeyCode::Down => {
            match e.position {
                Some(p) if p + 1 < e.historique.len() => {
                    e.position = Some(p + 1);
                    e.saisie = e.historique[p + 1].clone();
                }
                _ => {
                    e.position = None;
                    e.saisie.clear();
                }
            }
            None
        }
        KeyCode::Esc => {
            e.saisie.clear();
            None
        }
        KeyCode::Char(c) => {
            e.saisie.push(c);
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn un_texte_s_enveloppe_a_la_largeur() {
        assert_eq!(
            super::envelopper("un deux trois quatre", 10),
            ["un deux", "trois", "quatre"]
        );
        assert_eq!(
            super::envelopper("ligne\n\nautre", 20),
            ["ligne", "", "autre"]
        );
        assert_eq!(
            super::envelopper("abcdefghijklmnop", 10),
            ["abcdefghij", "klmnop"]
        );
    }

    #[test]
    fn un_outil_se_resume_par_son_argument_principal() {
        let a = serde_json::json!({"path": "index.html", "content": "…"});
        assert_eq!(
            super::resume_outil("write_file", &a),
            "write_file(index.html)"
        );
        assert_eq!(
            super::resume_outil("ask_user", &serde_json::json!({})),
            "ask_user"
        );
    }
}

#[cfg(test)]
mod rendu {
    use super::*;
    use ratatui::backend::TestBackend;

    #[test]
    #[ignore]
    fn montrer() {
        let mut t = Terminal::with_options(
            TestBackend::new(90, 34),
            TerminalOptions {
                viewport: Viewport::Inline(HAUTEUR),
            },
        )
        .unwrap();
        // Le même type que Term n'est pas requis pour ce rendu : on rejoue à la main.
        let e = Etat {
            agentic: true,
            modele: "Qwen3.5-9B-UD-IQ3_XXS".into(),
            dossier: r"C:\Users\pcfix\Documents\Locaryn\test-objectif-arcade".into(),
            session: "x".into(),
            saisie: String::new(),
            historique: vec![],
            position: None,
            reponse: String::new(),
            ecrit: 0,
            en_cours: true,
            voir_reflexion: false,
            activite: "Écrit index.html (2 431 car.)".into(),
            accord: None,
            toupie: 3,
            contexte: Some((7_400, 92_160)),
            detail_contexte: vec![],
            vitesse: Some(10.0),
            quitter: false,
        };
        let l = |p: &str, s: &str| {
            Line::from(vec![
                Span::styled(p.to_string(), Style::default().fg(ACCENT)),
                Span::raw(s.to_string()),
            ])
        };
        let lignes = vec![
            l("✻ ", "Bienvenue dans Locaryn  v0.4.36"),
            Line::from(""),
            l("> ", "crée un site arcade avec un snake"),
            Line::from(""),
            l("● ", "Je commence par la page d'accueil, puis le jeu."),
            l("⏺ ", "write_file(index.html)"),
            l("  ⎿ ", "Fichier écrit : index.html (2,4 Ko)"),
            l("⏺ ", "image_search(arcade cabinet)"),
            l("  ⎿ ", "Images brave pour « arcade cabinet » :"),
        ];
        let h = lignes.len() as u16;
        t.insert_before(h, |buf| Paragraph::new(lignes).render(buf.area, buf))
            .unwrap();
        t.draw(|f| dessiner(f, &e)).unwrap();
        let b = t.backend().buffer().clone();
        for y in 0..b.area.height {
            let mut s = String::new();
            for x in 0..b.area.width {
                s.push_str(b[(x, y)].symbol());
            }
            println!("{}", s.trim_end());
        }
        let mut e2 = e;
        e2.accord = Some(Accord {
            call_id: "c".into(),
            outil: "run_command".into(),
            raison: "Lance une commande dans le dossier du projet.".into(),
            resume: "run_command(npm test)".into(),
        });
        t.draw(|f| dessiner(f, &e2)).unwrap();
        let b = t.backend().buffer().clone();
        println!("---- accord");
        for y in 0..b.area.height {
            let mut s = String::new();
            for x in 0..b.area.width {
                s.push_str(b[(x, y)].symbol());
            }
            println!("{}", s.trim_end());
        }
    }
}
