//! Ce que la machine a, mesuré plutôt que supposé.
//!
//! La quantité de mémoire ne suffit pas à dire si un modèle « tourne » : à
//! taille égale, la même machine génère quarante jetons par seconde depuis la
//! VRAM et trois depuis la RAM. Ce qui décide, c'est la bande passante — et
//! elle, on peut la mesurer au lieu de la deviner.
//!
//! Les valeurs statiques (RAM totale, GPU, bande passante) sont sondées une
//! fois et gardées. La mémoire libre, elle, est relue régulièrement : c'est
//! précisément ce qui change entre le moment où l'utilisateur ouvre la liste
//! des modèles et celui où il en charge un. Elle garde tout de même quelques
//! secondes de validité, faute de quoi estimer une liste entière relancerait
//! une sonde système par ligne.

use serde::Serialize;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Ce qui exécute réellement les couches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Cuda,
    Metal,
    Rocm,
    Vulkan,
    Cpu,
}

impl Backend {
    pub fn label(self) -> &'static str {
        match self {
            Backend::Cuda => "CUDA",
            Backend::Metal => "Metal",
            Backend::Rocm => "ROCm",
            Backend::Vulkan => "Vulkan",
            Backend::Cpu => "processeur",
        }
    }
}

/// La machine, telle que l'estimateur la voit.
#[derive(Debug, Clone, Serialize)]
pub struct HardwareProfile {
    pub cpu_cores: u32,
    pub total_ram_gb: f64,
    pub free_ram_gb: f64,
    pub gpu_name: Option<String>,
    pub total_vram_gb: f64,
    pub free_vram_gb: f64,
    pub backend: Backend,
    /// Bande passante mémoire système, en Go/s.
    pub ram_bandwidth_gbps: f64,
    /// Bande passante de la mémoire graphique, en Go/s.
    pub vram_bandwidth_gbps: f64,
    /// Vrai quand la bande passante RAM vient d'une mesure sur cette machine,
    /// faux quand c'est une valeur par défaut.
    pub ram_bandwidth_measured: bool,
    /// Mémoire unifiée : la VRAM *est* la RAM, la partager ne coûte rien.
    pub unified_memory: bool,
}

impl HardwareProfile {
    /// Mémoire réellement utilisable par le GPU, réserve déduite.
    /// Le même profil, avec `gb` de mémoire vidéo rendue : celle que le moteur
    /// de l'application occupe et libérera en changeant de modèle. Sans elle,
    /// tout modèle paraît trop gros tant qu'un autre est chargé.
    pub fn with_reclaimed_vram(mut self, gb: f64) -> Self {
        if gb > 0.0 {
            self.free_vram_gb = (self.free_vram_gb + gb).min(self.total_vram_gb);
        }
        self
    }

    pub fn usable_vram_gb(&self, reserve_gb: f64) -> f64 {
        (self.free_vram_gb - reserve_gb).max(0.0)
    }

    /// Mémoire système réellement utilisable, réserve déduite.
    pub fn usable_ram_gb(&self, reserve_gb: f64) -> f64 {
        (self.free_ram_gb - reserve_gb).max(0.0)
    }

    /// Puissance de calcul utile, en opérations flottantes par seconde.
    ///
    /// Aucune API portable ne la donne. Mais sur toutes les cartes récentes,
    /// le rapport entre calcul et bande passante est remarquablement stable :
    /// une carte qui lit deux fois plus vite calcule à peu près deux fois plus
    /// vite. C'est ce rapport qu'on utilise, faute de mieux, et le rapport
    /// d'estimation le dit.
    pub fn gpu_flops(&self) -> f64 {
        if self.total_vram_gb <= 0.0 {
            return 0.0;
        }
        // ~0,15 TFLOP/s en demi-précision par Go/s de bande passante, mesuré
        // sur les générations RTX 30/40/50 et Apple M.
        self.vram_bandwidth_gbps * 0.15e12 * GPU_COMPUTE_EFFICIENCY
    }

    /// Idem côté processeur : cœurs physiques estimés × largeur AVX × horloge
    /// typique, ramenés au rendement réellement observé en inférence.
    pub fn cpu_flops(&self) -> f64 {
        let physical = if self.cpu_cores > 4 {
            self.cpu_cores as f64 / 2.0
        } else {
            self.cpu_cores as f64
        };
        physical * 3.2e9 * 16.0 * 2.0 * CPU_COMPUTE_EFFICIENCY
    }
}

/// Rendement réel du calcul GPU en inférence : le pic théorique n'est jamais
/// atteint sur des tenseurs quantifiés.
const GPU_COMPUTE_EFFICIENCY: f64 = 0.30;
/// Idem processeur, où la marge est plus mince encore.
const CPU_COMPUTE_EFFICIENCY: f64 = 0.25;

/// Bande passante supposée d'une mémoire système dont la mesure a échoué.
/// Choisie basse : mieux vaut annoncer une vitesse prudente qu'une promesse.
const RAM_BANDWIDTH_FALLBACK: f64 = 30.0;

// ============================================================================
// Sondage
// ============================================================================

/// Partie statique du profil : sondée une fois, gardée pour la session.
struct StaticProbe {
    cpu_cores: u32,
    total_ram_gb: f64,
    gpu_name: Option<String>,
    total_vram_gb: f64,
    backend: Backend,
    ram_bandwidth_gbps: f64,
    ram_bandwidth_measured: bool,
    vram_bandwidth_gbps: f64,
    unified_memory: bool,
}

static STATIC_PROBE: OnceLock<StaticProbe> = OnceLock::new();

/// Durée pendant laquelle une mesure de mémoire libre reste valable.
///
/// Chaque relecture lance un processus — PowerShell, `nvidia-smi`. Estimer les
/// trois cents lignes d'une liste de modèles en relançant ces sondes à chaque
/// ligne coûterait des minutes pour un résultat identique. Cinq secondes
/// suffisent à rester juste : la mémoire libre bouge, mais pas à cette
/// vitesse.
const LIVE_MEMORY_TTL: Duration = Duration::from_secs(5);

/// Dernière lecture de la mémoire libre : (instant, RAM, VRAM).
static LIVE_MEMORY: Mutex<Option<(Instant, f64, f64)>> = Mutex::new(None);

/// Le profil complet de la machine. Bloquant au premier appel (quelques
/// centaines de millisecondes : sondes système et mesure de bande passante),
/// immédiat ensuite hormis la lecture de la mémoire libre, rafraîchie toutes
/// les cinq secondes.
pub fn profile() -> HardwareProfile {
    let base = STATIC_PROBE.get_or_init(probe_static);
    let (free_ram, free_vram) = live_memory(base);
    HardwareProfile {
        cpu_cores: base.cpu_cores,
        total_ram_gb: base.total_ram_gb,
        free_ram_gb: free_ram,
        gpu_name: base.gpu_name.clone(),
        total_vram_gb: base.total_vram_gb,
        free_vram_gb: free_vram,
        backend: base.backend,
        ram_bandwidth_gbps: base.ram_bandwidth_gbps,
        vram_bandwidth_gbps: base.vram_bandwidth_gbps,
        ram_bandwidth_measured: base.ram_bandwidth_measured,
        unified_memory: base.unified_memory,
    }
}

/// Mémoire libre, relue au plus une fois toutes les `LIVE_MEMORY_TTL`.
fn live_memory(base: &StaticProbe) -> (f64, f64) {
    let mut cache = match LIVE_MEMORY.lock() {
        Ok(cache) => cache,
        // Un verrou empoisonné ne doit pas empêcher d'estimer : on sonde.
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some((measured_at, ram, vram)) = *cache {
        if measured_at.elapsed() < LIVE_MEMORY_TTL {
            return (ram, vram);
        }
    }
    let ram = free_ram_gb().unwrap_or(base.total_ram_gb * 0.5);
    let vram = if base.unified_memory {
        // Mémoire unifiée : ce qui est libre pour le système l'est pour le GPU.
        ram
    } else {
        free_vram_gb().unwrap_or(base.total_vram_gb * 0.9)
    };
    *cache = Some((Instant::now(), ram, vram));
    (ram, vram)
}

fn probe_static() -> StaticProbe {
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4);
    let total_ram_gb = total_ram_gb().unwrap_or(16.0);
    // Ce profil est gardé pour toute la session : une carte manquée au premier
    // essai — pilote qui se réveille, `nvidia-smi` lent au démarrage de la
    // machine — resterait « aucun GPU » jusqu'au prochain lancement. Un second
    // essai ne coûte qu'une fraction de seconde, et seulement quand le premier
    // n'a rien trouvé.
    let gpu = probe_gpu().or_else(|| {
        std::thread::sleep(Duration::from_millis(700));
        probe_gpu()
    });
    let unified = cfg!(target_os = "macos") && gpu.is_some();
    let (measured_bw, measured) = match sample_ram_bandwidth_gbps() {
        Some(bw) => (bw, true),
        None => (RAM_BANDWIDTH_FALLBACK, false),
    };

    let (gpu_name, total_vram_gb, backend) = match gpu {
        Some(g) => (Some(g.name), g.total_vram_gb, g.backend),
        None => (None, 0.0, Backend::Cpu),
    };
    let vram_bandwidth_gbps = if unified {
        // Sur mémoire unifiée, le GPU lit la même mémoire que le processeur —
        // plus vite que lui, mais pas dans une autre puce.
        apple_bandwidth(gpu_name.as_deref()).unwrap_or(measured_bw * 2.0)
    } else if total_vram_gb > 0.0 {
        gpu_bandwidth(gpu_name.as_deref(), total_vram_gb)
    } else {
        0.0
    };

    StaticProbe {
        cpu_cores,
        total_ram_gb,
        gpu_name,
        total_vram_gb: if unified { total_ram_gb } else { total_vram_gb },
        backend,
        ram_bandwidth_gbps: measured_bw,
        ram_bandwidth_measured: measured,
        vram_bandwidth_gbps,
        unified_memory: unified,
    }
}

/// Empêcher une console noire d'apparaître derrière chaque sonde sous Windows.
fn quiet(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = command;
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let mut command = Command::new(program);
    command.args(args);
    quiet(&mut command);
    let out = command.output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

// ── Mémoire système ────────────────────────────────────────────────────────

/// La mémoire du système, en kibioctets : (totale, disponible).
///
/// Lue par `GlobalMemoryStatusEx`, l'appel que Windows fournit pour cela, et
/// non plus par PowerShell. Lancer PowerShell pouvait échouer — machine
/// chargée, stratégie d'exécution, démarrage lent — et l'échec retombait en
/// silence sur une valeur par défaut : une machine de 24 Go s'annonçait à
/// 16 Go pour toute la session. Un appel système ne lance aucun processus et
/// ne connaît pas ces pannes-là.
#[cfg(target_os = "windows")]
fn os_memory_kb() -> Option<(f64, f64)> {
    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }
    extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
    }
    let mut status = MemoryStatusEx {
        length: std::mem::size_of::<MemoryStatusEx>() as u32,
        memory_load: 0,
        total_phys: 0,
        avail_phys: 0,
        total_page_file: 0,
        avail_page_file: 0,
        total_virtual: 0,
        avail_virtual: 0,
        avail_extended_virtual: 0,
    };
    // SAFETY : la structure est celle que l'API attend, avec sa taille posée
    // dans `length` comme la documentation l'exige ; l'appel ne fait qu'y
    // écrire.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    if ok == 0 || status.total_phys == 0 {
        return None;
    }
    Some((
        status.total_phys as f64 / 1024.0,
        status.avail_phys as f64 / 1024.0,
    ))
}

#[cfg(target_os = "windows")]
fn total_ram_gb() -> Option<f64> {
    os_memory_kb().map(|(total, _)| total / (1024.0 * 1024.0))
}

#[cfg(target_os = "windows")]
fn free_ram_gb() -> Option<f64> {
    os_memory_kb().map(|(_, free)| free / (1024.0 * 1024.0))
}

#[cfg(target_os = "linux")]
fn meminfo_kb(key: &str) -> Option<f64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    text.lines()
        .find(|l| l.starts_with(key))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

#[cfg(target_os = "linux")]
fn total_ram_gb() -> Option<f64> {
    meminfo_kb("MemTotal:").map(|kb| kb / (1024.0 * 1024.0))
}

#[cfg(target_os = "linux")]
fn free_ram_gb() -> Option<f64> {
    // MemAvailable, pas MemFree : le cache page est récupérable, l'ignorer
    // ferait refuser des chargements parfaitement possibles.
    meminfo_kb("MemAvailable:").map(|kb| kb / (1024.0 * 1024.0))
}

#[cfg(target_os = "macos")]
fn total_ram_gb() -> Option<f64> {
    let text = run("sysctl", &["-n", "hw.memsize"])?;
    let bytes: f64 = text.trim().parse().ok()?;
    Some(bytes / (1024.0 * 1024.0 * 1024.0))
}

#[cfg(target_os = "macos")]
fn free_ram_gb() -> Option<f64> {
    let text = run("vm_stat", &[])?;
    let page_size = 4096.0;
    let mut pages = 0.0;
    for line in text.lines() {
        // Libres, inactives, purgeables : ce que le système peut rendre.
        if line.starts_with("Pages free:")
            || line.starts_with("Pages inactive:")
            || line.starts_with("Pages purgeable:")
        {
            if let Some(value) = line.split(':').nth(1) {
                if let Ok(v) = value.trim().trim_end_matches('.').parse::<f64>() {
                    pages += v;
                }
            }
        }
    }
    Some(pages * page_size / (1024.0 * 1024.0 * 1024.0))
}

// ── Carte graphique ────────────────────────────────────────────────────────

struct Gpu {
    name: String,
    total_vram_gb: f64,
    backend: Backend,
}

fn probe_gpu() -> Option<Gpu> {
    if let Some(gpu) = probe_nvidia() {
        return Some(gpu);
    }
    #[cfg(target_os = "macos")]
    if let Some(gpu) = probe_apple() {
        return Some(gpu);
    }
    // Windows : ni `nvidia-smi` ni `rocm-smi` ne sont présents sur une machine
    // grand public — ROCm ne couvre pas les cartes Polaris comme la RX 570, et
    // n'est même pas distribué sous Windows. La carte y restait donc invisible :
    // « 0 Go de VRAM » dans l'analyse matérielle, et des modèles refusés ou
    // relégués au processeur. Le pilote, lui, publie la taille réelle.
    #[cfg(target_os = "windows")]
    if let Some(gpu) = probe_windows_adapters() {
        return Some(gpu);
    }
    probe_amd()
}

fn probe_nvidia() -> Option<Gpu> {
    let text = run(
        "nvidia-smi",
        &[
            "--query-gpu=name,memory.total",
            "--format=csv,noheader,nounits",
        ],
    )?;
    let line = text.lines().next()?;
    let mut parts = line.split(',');
    let name = parts.next()?.trim().to_string();
    let mib: f64 = parts.next()?.trim().parse().ok()?;
    Some(Gpu {
        name,
        total_vram_gb: mib / 1024.0,
        backend: Backend::Cuda,
    })
}

fn probe_amd() -> Option<Gpu> {
    let text = run("rocm-smi", &["--showproductname", "--showmeminfo", "vram"])?;
    let mut name = String::new();
    let mut total_bytes = 0.0;
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if name.is_empty() && lower.contains("card series") {
            if let Some(value) = line.split(':').next_back() {
                name = value.trim().to_string();
            }
        }
        if lower.contains("vram total memory") {
            if let Some(value) = line.split(':').next_back() {
                total_bytes = value.trim().parse().unwrap_or(0.0);
            }
        }
    }
    if total_bytes <= 0.0 {
        return None;
    }
    Some(Gpu {
        name: if name.is_empty() {
            "GPU AMD".to_string()
        } else {
            name
        },
        total_vram_gb: total_bytes / (1024.0 * 1024.0 * 1024.0),
        backend: Backend::Rocm,
    })
}

/// Sous Windows, la carte que le pilote déclare — lue dans le registre.
///
/// `Win32_VideoController` ne convient pas : son `AdapterRAM` est un entier
/// 32 bits que Windows plafonne à 4 Go — une carte de 8 Go s'y annonce à 4,
/// voire à 0 selon le pilote. `HardwareInformation.qwMemorySize`, publié par
/// le pilote dans sa clé de classe, est un entier 64 bits : la seule source
/// fiable et sans dépendance qui couvre AMD comme Intel.
#[cfg(target_os = "windows")]
fn probe_windows_adapters() -> Option<Gpu> {
    choisir_meilleure_carte(lire_cartes_registre())
}

/// Retenir la carte qui compte : celle qui a le plus de mémoire.
///
/// Un poste de bureau en déclare souvent deux — un processeur graphique
/// intégré et la carte dédiée. C'est la dédiée, la plus large, sur laquelle un
/// modèle tiendra ; prendre la première clé du registre choisirait au hasard.
/// Les pilotes logiciels (`Microsoft Basic Display Adapter`) n'ont pas de
/// mémoire propre et sont écartés.
#[cfg(any(target_os = "windows", test))]
fn choisir_meilleure_carte(cartes: Vec<(String, f64)>) -> Option<Gpu> {
    cartes
        .into_iter()
        .filter(|(_, go)| *go >= 0.5)
        .filter(|(nom, _)| {
            let bas = nom.to_ascii_lowercase();
            !bas.contains("basic display") && !bas.contains("render only")
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(name, total_vram_gb)| Gpu {
            backend: backend_pour_nom(&name),
            name,
            total_vram_gb,
        })
}

/// Le moteur qu'un nom de carte désigne sous Windows.
///
/// AMD et Intel n'ont pas d'autre voie : le runtime intégré embarque
/// `ggml-vulkan.dll`, qui les sert tous les deux. Une carte NVIDIA trouvée ici
/// (et non par `nvidia-smi`) a un pilote qui n'a pas répondu au sondage ; ce
/// libellé est informatif — il ne décide pas du lancement — mais il ne doit pas
/// annoncer « ROCm » sur une machine Windows où ROCm n'existe pas.
#[cfg(any(target_os = "windows", test))]
fn backend_pour_nom(nom: &str) -> Backend {
    let bas = nom.to_ascii_lowercase();
    if bas.contains("nvidia") || bas.contains("geforce") || bas.contains("quadro") {
        Backend::Cuda
    } else {
        Backend::Vulkan
    }
}

/// Les cartes installées, `(nom, Go)` : chaque sous-clé de la classe des cartes
/// d'affichage est une carte, `DriverDesc` son nom et
/// `HardwareInformation.qwMemorySize` sa mémoire dédiée en octets.
#[cfg(target_os = "windows")]
fn lire_cartes_registre() -> Vec<(String, f64)> {
    let racine = registre_wide(
        r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}",
    );
    let mut racine_cle: isize = 0;
    // SAFETY : la clé est celle que l'API attend ; le tampon vit le temps de
    // l'appel et la clé est refermée avant de rendre la main.
    if unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            racine.as_ptr(),
            0,
            KEY_READ,
            &mut racine_cle,
        )
    } != ERROR_SUCCESS
    {
        return Vec::new();
    }

    let mut cartes = Vec::new();
    let mut index = 0u32;
    loop {
        let mut sous_nom = [0u16; 256];
        let mut sous_nom_len = sous_nom.len() as u32;
        // SAFETY : tampons vivants, longueurs annoncées.
        let code = unsafe {
            RegEnumKeyExW(
                racine_cle,
                index,
                sous_nom.as_mut_ptr(),
                &mut sous_nom_len,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if code != ERROR_SUCCESS {
            break;
        }
        index += 1;

        let mut carte_cle: isize = 0;
        let sous_chemin = registre_wide(&String::from_utf16_lossy(
            &sous_nom[..sous_nom_len as usize],
        ));
        // SAFETY : la sous-clé est celle qu'on vient d'énumérer.
        if unsafe {
            RegOpenKeyExW(
                racine_cle,
                sous_chemin.as_ptr(),
                0,
                KEY_READ,
                &mut carte_cle,
            )
        } != ERROR_SUCCESS
        {
            continue;
        }
        let nom = registre_chaine(carte_cle, "DriverDesc");
        let octets = registre_entier(carte_cle, "HardwareInformation.qwMemorySize")
            .or_else(|| registre_entier(carte_cle, "HardwareInformation.MemorySize"));
        // SAFETY : clé précédemment ouverte.
        unsafe { RegCloseKey(carte_cle) };

        if let (Some(nom), Some(octets)) = (nom, octets) {
            if octets > 0 {
                cartes.push((nom, octets as f64 / (1024.0 * 1024.0 * 1024.0)));
            }
        }
    }
    // SAFETY : clé précédemment ouverte.
    unsafe { RegCloseKey(racine_cle) };
    cartes
}

/// Une chaîne UTF-16 terminée par zéro, pour les appels `…W` du registre.
#[cfg(target_os = "windows")]
fn registre_wide(texte: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt as _;
    std::ffi::OsStr::new(texte)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// La valeur chaîne d'une clé du registre, si elle existe et est bien une
/// chaîne.
#[cfg(target_os = "windows")]
fn registre_chaine(cle: isize, nom: &str) -> Option<String> {
    let nom_w = registre_wide(nom);
    let mut tampon = [0u16; 512];
    let mut taille = (tampon.len() * std::mem::size_of::<u16>()) as u32;
    let mut kind = 0u32;
    // SAFETY : le tampon et la taille annoncée correspondent ; l'appel n'écrit
    // que dans le tampon, jamais au-delà de la taille qu'on lui donne.
    let code = unsafe {
        RegQueryValueExW(
            cle,
            nom_w.as_ptr(),
            std::ptr::null_mut(),
            &mut kind,
            tampon.as_mut_ptr() as *mut u8,
            &mut taille,
        )
    };
    if code != ERROR_SUCCESS || kind != REG_SZ {
        return None;
    }
    let longueur = ((taille as usize) / std::mem::size_of::<u16>()).min(tampon.len());
    let fin = tampon[..longueur]
        .iter()
        .position(|c| *c == 0)
        .unwrap_or(longueur);
    Some(String::from_utf16_lossy(&tampon[..fin]))
}

/// La valeur entière d'une clé du registre, qu'elle soit publiée en 32 ou en
/// 64 bits.
#[cfg(target_os = "windows")]
fn registre_entier(cle: isize, nom: &str) -> Option<u64> {
    let nom_w = registre_wide(nom);
    let mut tampon = [0u8; 16];
    let mut taille = tampon.len() as u32;
    let mut kind = 0u32;
    // SAFETY : idem — écriture bornée par `taille`.
    let code = unsafe {
        RegQueryValueExW(
            cle,
            nom_w.as_ptr(),
            std::ptr::null_mut(),
            &mut kind,
            tampon.as_mut_ptr(),
            &mut taille,
        )
    };
    if code != ERROR_SUCCESS {
        return None;
    }
    match kind {
        REG_QWORD if taille >= 8 => Some(u64::from_le_bytes(tampon[..8].try_into().ok()?)),
        REG_DWORD if taille >= 4 => Some(u32::from_le_bytes(tampon[..4].try_into().ok()?) as u64),
        // Certains pilotes publient `MemorySize` en binaire brut de 4 octets.
        REG_BINARY if taille >= 4 => Some(u32::from_le_bytes(tampon[..4].try_into().ok()?) as u64),
        _ => None,
    }
}

/// Les constantes et appels du registre Windows, sans dépendance externe.
#[cfg(target_os = "windows")]
const HKEY_LOCAL_MACHINE: isize = 0x8000_0002u32 as i32 as isize;
#[cfg(target_os = "windows")]
const KEY_READ: u32 = 0x2_0019;
#[cfg(target_os = "windows")]
const ERROR_SUCCESS: i32 = 0;
#[cfg(target_os = "windows")]
const REG_SZ: u32 = 1;
#[cfg(target_os = "windows")]
const REG_BINARY: u32 = 3;
#[cfg(target_os = "windows")]
const REG_DWORD: u32 = 4;
#[cfg(target_os = "windows")]
const REG_QWORD: u32 = 11;

#[cfg(target_os = "windows")]
#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(
        hkey: isize,
        sub_key: *const u16,
        options: u32,
        sam: u32,
        result: *mut isize,
    ) -> i32;
    fn RegEnumKeyExW(
        hkey: isize,
        index: u32,
        name: *mut u16,
        name_len: *mut u32,
        reserved: *mut u32,
        class: *mut u16,
        class_len: *mut u32,
        last_write: *mut core::ffi::c_void,
    ) -> i32;
    fn RegQueryValueExW(
        hkey: isize,
        value_name: *const u16,
        reserved: *mut u32,
        kind: *mut u32,
        data: *mut u8,
        data_len: *mut u32,
    ) -> i32;
    fn RegCloseKey(hkey: isize) -> i32;
}

#[cfg(target_os = "macos")]
fn probe_apple() -> Option<Gpu> {
    let brand = run("sysctl", &["-n", "machdep.cpu.brand_string"])?;
    let brand = brand.trim();
    if !brand.starts_with("Apple") {
        return None;
    }
    // Mémoire unifiée : la taille est celle de la RAM, renseignée par
    // l'appelant. On ne remonte ici que l'identité de la puce.
    Some(Gpu {
        name: brand.to_string(),
        total_vram_gb: total_ram_gb().unwrap_or(0.0),
        backend: Backend::Metal,
    })
}

/// Mémoire graphique libre, en Go. Seul NVIDIA la publie de façon fiable.
pub fn free_vram_gb() -> Option<f64> {
    let text = run(
        "nvidia-smi",
        &["--query-gpu=memory.free", "--format=csv,noheader,nounits"],
    )?;
    let mib: f64 = text.lines().next()?.trim().parse().ok()?;
    Some(mib / 1024.0)
}

// ── Bande passante ─────────────────────────────────────────────────────────

/// Bande passante d'une carte, en Go/s.
///
/// Table volontairement courte : les cartes qu'on croise réellement, et un
/// repli qui reste dans le bon ordre de grandeur pour les autres. Une valeur
/// approchée à 20 % près suffit à distinguer « quarante jetons par seconde »
/// de « trois ».
fn gpu_bandwidth(name: Option<&str>, total_vram_gb: f64) -> f64 {
    let lower = name.unwrap_or_default().to_ascii_lowercase();
    let table: &[(&str, f64)] = &[
        ("5090", 1792.0),
        ("5080", 960.0),
        ("5070 ti", 896.0),
        ("5070", 672.0),
        ("5060", 448.0),
        ("4090", 1008.0),
        ("4080", 717.0),
        ("4070 ti", 504.0),
        ("4070", 504.0),
        ("4060 ti", 288.0),
        ("4060", 272.0),
        ("4050", 192.0),
        ("3090", 936.0),
        ("3080", 760.0),
        ("3070", 448.0),
        ("3060", 360.0),
        ("3050", 224.0),
        ("2080", 448.0),
        ("2060", 336.0),
        ("a100", 1935.0),
        ("h100", 3350.0),
        ("l40", 864.0),
        ("a6000", 768.0),
        ("a4000", 448.0),
        ("t4", 320.0),
        ("7900 xtx", 960.0),
        ("7900", 800.0),
        ("7800", 624.0),
        ("7700", 432.0),
        ("6900", 512.0),
        ("6800", 512.0),
        ("6700", 384.0),
        ("arc a770", 560.0),
        ("arc b580", 456.0),
        // Polaris, Vega et Navi : les cartes AMD qu'on croise encore. Le repli
        // « mémoire × 45 » les surestimait largement — une RX 570 de 8 Go lit à
        // 224 Go/s, pas à 360. Les 5700 sont listées avant les 570 : « rx 570 »
        // est un préfixe de « rx 5700 », et l'ordre de cette table décide.
        ("rx 5700 xt", 448.0),
        ("rx 5700", 448.0),
        ("rx 5600", 336.0),
        ("rx 5500", 224.0),
        ("rx 590", 256.0),
        ("rx 580", 256.0),
        ("rx 570", 224.0),
        ("rx 480", 256.0),
        ("rx 470", 211.0),
        ("radeon vii", 1024.0),
        ("vega 64", 484.0),
        ("vega 56", 410.0),
    ];
    for (needle, bandwidth) in table {
        if lower.contains(needle) {
            return *bandwidth;
        }
    }
    // Repli : la bande passante suit grossièrement la quantité de mémoire
    // embarquée, parce que les deux suivent la largeur du bus.
    (total_vram_gb * 45.0).clamp(100.0, 1200.0)
}

/// Bande passante d'une puce Apple, où mémoire système et mémoire graphique
/// sont la même chose.
fn apple_bandwidth(name: Option<&str>) -> Option<f64> {
    let lower = name?.to_ascii_lowercase();
    let table: &[(&str, f64)] = &[
        ("m4 max", 546.0),
        ("m4 pro", 273.0),
        ("m4", 120.0),
        ("m3 ultra", 800.0),
        ("m3 max", 400.0),
        ("m3 pro", 150.0),
        ("m3", 100.0),
        ("m2 ultra", 800.0),
        ("m2 max", 400.0),
        ("m2 pro", 200.0),
        ("m2", 100.0),
        ("m1 ultra", 800.0),
        ("m1 max", 400.0),
        ("m1 pro", 200.0),
        ("m1", 68.0),
    ];
    table
        .iter()
        .find(|(needle, _)| lower.contains(needle))
        .map(|(_, bandwidth)| *bandwidth)
}

/// Mesurer la bande passante mémoire de cette machine, en Go/s.
///
/// On lit un tampon plus grand que n'importe quel cache de dernier niveau, en
/// parallèle sur plusieurs fils : c'est exactement ce que fait l'inférence
/// quand elle traverse les poids à chaque jeton. Le meilleur de trois passes
/// est retenu — un système chargé fausse une mesure isolée, jamais les trois
/// dans le même sens.
pub fn sample_ram_bandwidth_gbps() -> Option<f64> {
    const BUFFER_BYTES: usize = 128 * 1024 * 1024;
    const PASSES: usize = 3;

    let words = BUFFER_BYTES / std::mem::size_of::<u64>();
    let mut buffer: Vec<u64> = Vec::new();
    buffer.try_reserve_exact(words).ok()?;
    // Remplir vraiment : un tampon jamais écrit peut n'être pas encore mappé,
    // et on mesurerait alors des défauts de page, pas de la mémoire.
    buffer.extend((0..words).map(|i| i as u64));

    let threads = std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4)
        .max(1);
    let chunk = words / threads;
    if chunk == 0 {
        return None;
    }

    let mut best_gbps = 0.0f64;
    for _ in 0..PASSES {
        let start = Instant::now();
        std::thread::scope(|scope| {
            for slice in buffer.chunks(chunk) {
                scope.spawn(move || {
                    let mut sum = 0u64;
                    // Pas de 8 mots : une ligne de cache par itération, ce que
                    // le préchargeur matériel sert au débit maximal.
                    for value in slice.iter().step_by(8) {
                        sum = sum.wrapping_add(*value);
                    }
                    std::hint::black_box(sum);
                });
            }
        });
        let seconds = start.elapsed().as_secs_f64();
        if seconds > 0.0 {
            let gbps = (BUFFER_BYTES as f64 / 1e9) / seconds;
            best_gbps = best_gbps.max(gbps);
        }
    }

    // Une mesure absurde (machine gelée, minuteur imprécis) ne vaut pas mieux
    // que pas de mesure du tout.
    if (2.0..=2000.0).contains(&best_gbps) {
        Some(best_gbps)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La mémoire se lit par l'appel système, sans PowerShell : une valeur
    /// réelle, jamais le repli de 16 Go qu'un processus en échec laissait
    /// passer. Le chiffre est imprimé pour qu'on puisse le comparer à la
    /// machine qui fait tourner le test.
    #[cfg(target_os = "windows")]
    #[test]
    fn la_memoire_se_lit_par_l_appel_systeme() {
        let (total_kb, libre_kb) = os_memory_kb().expect("GlobalMemoryStatusEx a échoué");
        let total_go = total_kb / (1024.0 * 1024.0);
        let libre_go = libre_kb / (1024.0 * 1024.0);
        eprintln!("mémoire : {total_go:.1} Go au total, {libre_go:.1} Go disponibles");
        assert!(total_go > 1.0);
        assert!(libre_go > 0.0 && libre_go <= total_go);
    }

    /// Le sondage ne doit jamais rendre un profil incohérent, même sur une
    /// machine où toutes les sondes échouent.
    #[test]
    fn profil_toujours_coherent() {
        let hw = profile();
        assert!(hw.cpu_cores >= 1);
        assert!(hw.total_ram_gb > 0.0);
        assert!(hw.free_ram_gb >= 0.0);
        assert!(hw.free_ram_gb <= hw.total_ram_gb * 1.05);
        assert!(hw.ram_bandwidth_gbps > 0.0);
    }

    /// Le second appel doit être servi par le cache : sans lui, estimer une
    /// liste de modèles relancerait une sonde système par ligne.
    #[test]
    fn la_memoire_libre_est_mise_en_cache() {
        let premier = profile();
        let debut = std::time::Instant::now();
        let second = profile();
        assert!(
            debut.elapsed() < LIVE_MEMORY_TTL,
            "le second appel ne doit pas re-sonder la machine"
        );
        assert_eq!(premier.free_ram_gb, second.free_ram_gb);
    }

    /// Une machine qui déclare un processeur graphique intégré et une carte
    /// dédiée doit retenir la seconde : c'est la seule sur laquelle un modèle
    /// tient, et l'ancien sondage `rocm-smi` n'en trouvait aucune des deux.
    #[test]
    fn la_carte_la_plus_large_est_retenue() {
        let cartes = vec![
            ("Intel(R) UHD Graphics 630".to_string(), 1.0),
            ("AMD Radeon RX 570 Series".to_string(), 8.0),
        ];
        let gpu = choisir_meilleure_carte(cartes).expect("une carte");
        assert_eq!(gpu.total_vram_gb, 8.0);
        assert_eq!(gpu.backend, Backend::Vulkan);
        assert!(gpu.name.contains("RX 570"));
    }

    /// Un adaptateur logiciel sans mémoire propre ne doit pas devenir « le GPU »
    /// de la machine.
    #[test]
    fn l_adaptateur_logiciel_n_est_pas_un_gpu() {
        let cartes = vec![
            ("Microsoft Basic Display Adapter".to_string(), 0.0),
            ("AMD Radeon RX 570 Series".to_string(), 8.0),
        ];
        assert!(choisir_meilleure_carte(cartes).is_some_and(|g| g.total_vram_gb == 8.0));
        assert!(choisir_meilleure_carte(vec![(
            "Microsoft Basic Display Adapter".to_string(),
            0.0
        )])
        .is_none());
    }

    /// Une RX 570 annonce sa vraie bande passante (~224 Go/s), pas le repli
    /// générique qui la surestimerait de 60 %.
    #[test]
    fn la_bande_passante_d_une_polaris_est_connue() {
        assert_eq!(gpu_bandwidth(Some("AMD Radeon RX 570 Series"), 8.0), 224.0);
        // « RX 5700 » ne doit pas être prise pour la « RX 570 ».
        assert_eq!(gpu_bandwidth(Some("AMD Radeon RX 5700 XT"), 8.0), 448.0);
    }

    /// Une carte connue doit sortir sa vraie bande passante, une inconnue un
    /// ordre de grandeur plausible plutôt que zéro.
    #[test]
    fn bande_passante_des_cartes() {
        assert_eq!(gpu_bandwidth(Some("NVIDIA GeForce RTX 4090"), 24.0), 1008.0);
        assert_eq!(
            gpu_bandwidth(Some("NVIDIA GeForce RTX 4050 Laptop GPU"), 6.0),
            192.0
        );
        let inconnue = gpu_bandwidth(Some("Carte du futur"), 16.0);
        assert!((100.0..=1200.0).contains(&inconnue));
    }

    /// La mesure doit tomber dans un intervalle physiquement possible : une
    /// valeur hors bornes signale un bug de chronométrage, pas un ordinateur
    /// exceptionnel.
    #[test]
    fn mesure_de_bande_passante_plausible() {
        if let Some(gbps) = sample_ram_bandwidth_gbps() {
            assert!((2.0..=2000.0).contains(&gbps), "obtenu {gbps} Go/s");
        }
    }

    /// Le calcul dérivé de la bande passante doit rester nul sans GPU, sinon
    /// l'estimateur promettrait une vitesse GPU sur une machine qui n'en a pas.
    #[test]
    fn pas_de_gpu_pas_de_calcul() {
        let hw = HardwareProfile {
            cpu_cores: 8,
            total_ram_gb: 16.0,
            free_ram_gb: 8.0,
            gpu_name: None,
            total_vram_gb: 0.0,
            free_vram_gb: 0.0,
            backend: Backend::Cpu,
            ram_bandwidth_gbps: 40.0,
            vram_bandwidth_gbps: 0.0,
            ram_bandwidth_measured: true,
            unified_memory: false,
        };
        assert_eq!(hw.gpu_flops(), 0.0);
        assert!(hw.cpu_flops() > 0.0);
    }
}
