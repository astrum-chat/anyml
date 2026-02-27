/// Trait for values that can be written as JSON into a string buffer.
/// Used by the `json_string!` macro for runtime variable interpolation.
pub trait JsonValue {
    fn write_json(&self, buf: &mut String);
}

impl<T: JsonValue + ?Sized> JsonValue for &T {
    fn write_json(&self, buf: &mut String) {
        (*self).write_json(buf);
    }
}

impl<T: JsonValue + ?Sized> JsonValue for &mut T {
    fn write_json(&self, buf: &mut String) {
        (**self).write_json(buf);
    }
}

impl JsonValue for &str {
    fn write_json(&self, buf: &mut String) {
        buf.push('"');
        json_escape_into(self, buf);
        buf.push('"');
    }
}

impl JsonValue for String {
    fn write_json(&self, buf: &mut String) {
        buf.push('"');
        json_escape_into(self, buf);
        buf.push('"');
    }
}

impl JsonValue for bool {
    fn write_json(&self, buf: &mut String) {
        buf.push_str(if *self { "true" } else { "false" });
    }
}

impl JsonValue for usize {
    fn write_json(&self, buf: &mut String) {
        buf.push_str(&self.to_string());
    }
}

impl JsonValue for u32 {
    fn write_json(&self, buf: &mut String) {
        buf.push_str(&self.to_string());
    }
}

impl JsonValue for u64 {
    fn write_json(&self, buf: &mut String) {
        buf.push_str(&self.to_string());
    }
}

impl JsonValue for i32 {
    fn write_json(&self, buf: &mut String) {
        buf.push_str(&self.to_string());
    }
}

impl JsonValue for i64 {
    fn write_json(&self, buf: &mut String) {
        buf.push_str(&self.to_string());
    }
}

impl JsonValue for f32 {
    fn write_json(&self, buf: &mut String) {
        buf.push_str(&self.to_string());
    }
}

impl JsonValue for f64 {
    fn write_json(&self, buf: &mut String) {
        buf.push_str(&self.to_string());
    }
}

fn json_escape_into(s: &str, buf: &mut String) {
    for ch in s.chars() {
        match ch {
            '"' => buf.push_str("\\\""),
            '\\' => buf.push_str("\\\\"),
            '\n' => buf.push_str("\\n"),
            '\r' => buf.push_str("\\r"),
            '\t' => buf.push_str("\\t"),
            c if c < '\x20' => {
                buf.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => buf.push(c),
        }
    }
}
