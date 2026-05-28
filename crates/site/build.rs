use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=templates/dashboard.css");
    println!("cargo:rerun-if-changed=templates/dashboard.js");
    println!("cargo:rerun-if-changed=templates/finder.js");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR must be set"));

    let css = fs::read_to_string("templates/dashboard.css").expect("read dashboard css");
    fs::write(out_dir.join("dashboard.css.min"), minify_css(&css)).expect("write minified css");
    for script in ["dashboard", "finder"] {
        let source = fs::read_to_string(format!("templates/{script}.js")).expect("read js");
        fs::write(out_dir.join(format!("{script}.js.min")), minify_js(&source))
            .expect("write minified js");
    }
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
