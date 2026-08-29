//! Le rapport en clair, depuis un terminal.
//!
//! Utile pour vérifier une estimation sur une vraie machine sans passer par
//! l'interface : `cargo run -p locaryn-llmfit --example rapport -- <fichier.gguf> [contexte]`
//! ou, pour un modèle pas encore téléchargé,
//! `cargo run -p locaryn-llmfit --example rapport -- 8B Q4_K_M`.

use locaryn_llmfit::{for_catalog, for_file, profile, RunOptions};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage : rapport <fichier.gguf> [contexte] | rapport <paramètres>B [quant]");
        std::process::exit(2);
    }

    let hardware = profile();
    println!(
        "Machine : {:.0} Go de RAM ({:.0} libres) à {:.0} Go/s{}",
        hardware.total_ram_gb,
        hardware.free_ram_gb,
        hardware.ram_bandwidth_gbps,
        if hardware.ram_bandwidth_measured {
            " (mesurés)"
        } else {
            " (supposés)"
        }
    );
    if let Some(gpu) = &hardware.gpu_name {
        println!(
            "GPU     : {gpu} — {:.1} Go de VRAM ({:.1} libres) à {:.0} Go/s en {}",
            hardware.total_vram_gb,
            hardware.free_vram_gb,
            hardware.vram_bandwidth_gbps,
            hardware.backend.label()
        );
    } else {
        println!("GPU     : aucun détecté");
    }
    println!();

    let path = Path::new(&args[0]);
    let report = if path.exists() {
        let options = RunOptions {
            context: args.get(1).and_then(|c| c.parse().ok()).unwrap_or(0),
            ..RunOptions::default()
        };
        for_file(path, &options)
    } else {
        let params: f64 = args[0]
            .trim_end_matches(['B', 'b'])
            .parse()
            .unwrap_or_else(|_| {
                eprintln!(
                    "« {} » n'est ni un fichier ni un nombre de milliards",
                    args[0]
                );
                std::process::exit(2);
            });
        for_catalog(
            &args[0],
            params,
            args.get(1).map(String::as_str),
            &RunOptions::default(),
        )
    };

    println!("{} — {}", report.model, report.message);
    println!();
    println!(
        "  verdict     {:?} ({})",
        report.verdict,
        report.placement.label()
    );
    println!("  quant       {}", report.quant);
    println!("  poids       {:.2} Go", report.weights_gb);
    println!(
        "  cache KV    {:.2} Go pour {} jetons",
        report.kv_cache_gb, report.context
    );
    println!("  calcul      {:.2} Go", report.compute_gb);
    println!("  total       {:.2} Go", report.required_gb);
    println!(
        "  couches     {}/{} sur le GPU",
        report.gpu_layers, report.total_layers
    );
    println!("  génération  {:.1} jetons/s", report.tokens_per_second);
    println!(
        "  prompt      {:.0} jetons/s",
        report.prompt_tokens_per_second
    );
    println!("  ctx max GPU {}", report.max_gpu_context);
    println!("  ctx max      {}", report.max_context);
    if let Some(quant) = &report.suggested_quant {
        println!("  alternative {quant}");
    }
    println!();
    for line in &report.assumptions {
        println!("  · {line}");
    }
}
