use std::{collections::BTreeSet, env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=templates/dashboard-glyphs.txt");
    println!("cargo:rerun-if-changed=templates/dashboard.css");
    println!("cargo:rerun-if-changed=templates/dashboard.js");
    println!("cargo:rerun-if-changed=templates/finder.js");
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

    let css = fs::read_to_string("templates/dashboard.css").expect("read dashboard css");
    fs::write(out_dir.join("dashboard.css.min"), minify_css(&css)).expect("write minified css");
    for script in ["dashboard", "finder"] {
        let source = fs::read_to_string(format!("templates/{script}.js")).expect("read js");
        fs::write(out_dir.join(format!("{script}.js.min")), minify_js(&source))
            .expect("write minified js");
    }
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

fn minify_css(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut string = None;
    let mut pending_space = false;

    while let Some(ch) = chars.next() {
        if let Some(quote) = string {
            out.push(ch);
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if ch == quote {
                string = None;
            }
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(next) = chars.next() {
                if next == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
            pending_space = true;
            continue;
        }
        if ch == '\'' || ch == '"' {
            if pending_space && needs_css_space(out.chars().last(), ch) {
                out.push(' ');
            }
            pending_space = false;
            string = Some(ch);
            out.push(ch);
            continue;
        }
        if ch.is_whitespace() {
            pending_space = true;
            continue;
        }
        if is_css_punctuation(ch) {
            trim_space(&mut out);
            out.push(ch);
            pending_space = false;
            continue;
        }
        if pending_space && needs_css_space(out.chars().last(), ch) {
            out.push(' ');
        }
        pending_space = false;
        out.push(ch);
    }
    out.trim().to_string()
}

fn minify_js(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    let mut string = None;
    let mut pending_space = false;

    while let Some(ch) = chars.next() {
        if let Some(quote) = string {
            out.push(ch);
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if ch == quote {
                string = None;
            }
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            for next in chars.by_ref() {
                if next == '\n' || next == '\r' {
                    break;
                }
            }
            pending_space = true;
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(next) = chars.next() {
                if next == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
            }
            pending_space = true;
            continue;
        }
        if ch == '\'' || ch == '"' || ch == '`' {
            if pending_space && needs_js_space(out.chars().last(), ch) {
                out.push(' ');
            }
            pending_space = false;
            string = Some(ch);
            out.push(ch);
            continue;
        }
        if ch.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && needs_js_space(out.chars().last(), ch) {
            out.push(' ');
        }
        pending_space = false;
        out.push(ch);
    }
    out.trim().to_string()
}

fn trim_space(value: &mut String) {
    while value.ends_with(' ') {
        value.pop();
    }
}

fn is_css_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '{' | '}' | ':' | ';' | ',' | '>' | '+' | '~' | '(' | ')'
    )
}

fn needs_css_space(previous: Option<char>, current: char) -> bool {
    previous
        .map(|previous| is_ident_char(previous) && is_ident_char(current))
        .unwrap_or(false)
}

fn needs_js_space(previous: Option<char>, current: char) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    (is_ident_char(previous) && is_ident_char(current))
        || (is_ident_char(previous) && matches!(current, '[' | '(' | '{' | '\'' | '"' | '`'))
        || (matches!(previous, '+' | '-') && previous == current)
}

fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' || ch == '-'
}
