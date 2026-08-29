pub fn step_str(s: &str) -> String {
    let mut out = String::from("'");
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("''");
        } else if ch.is_ascii() && !ch.is_control() {
            out.push(ch);
        } else {
            out.push_str(&format!("\\X2\\{:04X}\\X0\\", ch as u32));
        }
    }
    out.push('\'');
    out
}

pub fn format_real(v: f64) -> String {
    let mut s = format!("{v:.6}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            // keep the trailing dot (ISO 10303-21 real)
        }
    }
    s
}

pub struct Ids(u32);

impl Ids {
    pub fn new() -> Self {
        Self(1)
    }
    pub fn next(&mut self) -> u32 {
        let n = self.0;
        self.0 += 1;
        n
    }
}
