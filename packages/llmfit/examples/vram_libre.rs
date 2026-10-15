//! La VRAM libre mesurée maintenant, et ce que la prudence décide pour un
//! modèle : `cargo run -p locaryn-llmfit --example vram_libre -- <modele.gguf>`.

use locaryn_llmfit::{for_file_on, profile, Headroom, KvType, RunOptions};

fn main() {
    let p = profile();
    println!(
        "VRAM libre : {:.2} Go sur {:.2} Go",
        p.free_vram_gb, p.total_vram_gb
    );
    let Some(modele) = std::env::args().nth(1) else {
        return;
    };
    for libre in [p.free_vram_gb, 5.0, 4.5, 4.0] {
        let mut machine = p.clone();
        machine.free_vram_gb = libre;
        let options = RunOptions {
            context: 8192,
            kv_type: KvType::Q8_0,
            flash_attention: true,
            headroom: Headroom::Prudent,
            strict_vram: true,
            ..RunOptions::default()
        };
        let r = for_file_on(std::path::Path::new(&modele), &options, &machine);
        println!(
            "{libre:.2} Go libres → {:?} (réserve {:.2} Go) : {}",
            r.verdict, r.vram_reserve_gb, r.message
        );
    }
}
