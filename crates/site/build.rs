use std::{collections::BTreeSet, env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=templates/dashboard-glyphs.txt");
    println!("cargo:rerun-if-env-changed=ATKINSON_FONT");

    let source_font = env::var("ATKINSON_FONT")
        .expect("ATKINSON_FONT must point to AtkinsonHyperlegible-Regular.otf");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be set"));
    let glyphs =
        fs::read_to_string("templates/dashboard-glyphs.txt").expect("read dashboard glyph list");
    let unicodes = glyphs
        .chars()
        .filter(|ch| !ch.is_control())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|ch| format!("U+{:04X}", ch as u32))
        .collect::<Vec<_>>()
        .join(",");
    let font_out = out_dir.join("dashboard-font.woff2");
    let status = Command::new("pyftsubset")
        .arg(source_font)
        .arg(format!("--output-file={}", font_out.display()))
        .arg("--flavor=woff2")
        .arg(format!("--unicodes={unicodes}"))
        .arg("--no-hinting")
        .arg("--layout-features=*")
        .status()
        .expect("run pyftsubset");
    if !status.success() {
        panic!("pyftsubset failed with {status}");
    }

    let bytes = fs::read(font_out).expect("read subset font");
    fs::write(out_dir.join("dashboard-font.woff2.b64"), base64(&bytes))
        .expect("write base64 subset font");
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = *chunk.get(1).unwrap_or(&0);
        let third = *chunk.get(2).unwrap_or(&0);
        encoded.push(TABLE[(first >> 2) as usize] as char);
        encoded.push(TABLE[(((first & 0b0000_0011) << 4) | (second >> 4)) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            TABLE[(((second & 0b0000_1111) << 2) | (third >> 6)) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            TABLE[(third & 0b0011_1111) as usize] as char
        } else {
            '='
        });
    }
    encoded
}
