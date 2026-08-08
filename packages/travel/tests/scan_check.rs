//! Writes a rendered code where an external scanner can look at it.
//!
//! Ignored: it exists so the encoding can be checked against a real reader
//! rather than against our own assumptions about it.
//!
//! ```text
//! LOCHOR_QR_OUT=/tmp/qr.txt cargo test -p lochor-travel --test scan_check -- --ignored
//! ```
#[test]
#[ignore = "writes a file for an external scanner"]
fn emit_a_code_for_a_real_reader() {
    let link = "lochor://travel?v=1&m=travel&u=aHR0cHM6Ly9wZXRpdGUtY2hvc2UtYWJjZC0xMjM0LnRyeWNsb3VkZmxhcmUuY29t&e=1800000600&k=AbCdEfGhIj&s=MEUCIQDxYnVzZWQtc2lnbmF0dXJlLWJ5dGVzLWhlcmU";
    let (w, m) = lochor_travel::qr::matrix(link).unwrap();
    let mut out = format!("{link}\n{w}\n");
    for y in 0..w {
        for x in 0..w {
            out.push(if m[y * w + x] { '1' } else { '0' });
        }
        out.push('\n');
    }
    let path = std::env::var("LOCHOR_QR_OUT").expect("LOCHOR_QR_OUT");
    std::fs::write(&path, out).unwrap();
    std::fs::write(format!("{path}.term"), lochor_travel::qr::terminal(link).unwrap()).unwrap();
}
