# Third-party licenses

Locaryn bundles and depends on third-party software. This directory collects the
license notices we are **required to redistribute**, and records the compliance
policy for shipping a commercial (paid) build.

> This is engineering compliance, not legal advice. A lawyer should sign off
> before charging enterprises — this folder is the dossier that makes that easy.

## TL;DR — is the stack safe to sell?

**Yes.** Every dependency and bundled engine is under a **permissive** license
(MIT / Apache-2.0 / BSD / ISC / Zlib / Unicode / MPL-2.0). There is **no
GPL / AGPL / SSPL** anywhere in the tree, so a closed-source, paid product is
allowed. Two things are required to stay compliant:

1. **Ship the notices** in this folder with every distribution (installer,
   `.deb`, `.exe`, `.dmg`). See "Bundled binaries" below.
2. **Do not modify the MPL-2.0 files** in-place (see "Weak copyleft").

The **models** are a separate matter and are intentionally **not** bundled —
see "Models" at the bottom.

## Audit results (reproducible via `scripts/license-audit.*`)

| Surface | Crates/pkgs | Result |
| --- | --- | --- |
| Rust workspace (`cargo metadata`) | 708 third-party | ✅ all permissive; 0 GPL/AGPL; 0 undeclared |
| Frontend prod deps (`pnpm licenses`) | React + Tauri set | ✅ all MIT / Apache-2.0 |
| Bundled binaries | llama.cpp, stable-diffusion.cpp, ggml | ✅ MIT — notices in this folder |

### Notable / false alarms

- **`r-efi`** (`MIT OR Apache-2.0 OR LGPL-2.1-or-later`) — this is a
  **disjunctive** SPDX expression. We take the **MIT** option; the LGPL option
  is never exercised. `cargo-deny` resolves this automatically.
- **MPL-2.0** (weak copyleft, *file*-level): `cssparser`, `cssparser-macros`,
  `dtoa-short`, `option-ext`, `selectors` (pulled in by the CSS/URL stack).
  Allowed in a proprietary product **as long as those files are used
  unmodified**. If you ever patch one of them, you must publish the changes to
  *that file only* (not your whole app).

## Bundled binaries (redistributed as-is)

These ship inside the app (`data_dir/bin/llama`, `data_dir/bin/sd`) and their
MIT notices **must** travel with the distribution:

| Engine | Upstream | License | File |
| --- | --- | --- | --- |
| llama.cpp / ggml | github.com/ggml-org/llama.cpp | MIT | `llama.cpp.LICENSE.txt` |
| stable-diffusion.cpp | github.com/leejet/stable-diffusion.cpp | MIT | `stable-diffusion.cpp.LICENSE.txt` |

The desktop build should surface this folder in an **About → Open-source
licenses** screen and copy it into the installer payload.

## Future: Python model-editing sidecar

When the optional fine-tuning / distillation / abliteration sidecar lands, it
adds (all commercial-friendly):

| Library | License |
| --- | --- |
| PyTorch | BSD-3-Clause |
| HuggingFace PEFT / TRL | Apache-2.0 |
| Unsloth (OSS) | Apache-2.0 |
| transformer-lens | MIT |

Re-run `scripts/license-audit.*` (it includes a `pip-licenses` pass) after the
sidecar's `requirements.txt` exists. **Watch for GPL** — some ML utilities are
GPL; the audit will flag them before they ship.

## Models

Locaryn **does not bundle model weights**. Users download them (HuggingFace,
etc.) at their own initiative, and each model carries its **own** license and
acceptable-use policy — some are restrictive:

- **Llama** (Meta): custom license (MAU cap, naming, AUP)
- **Gemma** (Google): custom license with use restrictions
- some **Qwen**: Tongyi Qianwen license

The marketplace must display each model's license and never redistribute
restricted weights. This keeps the model-license relationship strictly between
the end user and the model provider.
