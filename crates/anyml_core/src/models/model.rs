use phf::phf_map;

static REPLACEMENT_WORDS: phf::Map<&'static str, &'static str> = phf_map! {
    "ai" => "AI",
    "api" => "API",
    "gpt" => "GPT",
    "lm" => "LM",
    "llm" => "LLM",
    "moe" => "MoE",       // Mixture of Experts
    "oss" => "OSS",
    "sd" => "SD",         // Stable Diffusion
    "sdxl" => "SDXL",     // Stable Diffusion XL
    "vlm" => "VLM",       // Vision Language Model
    "xl" => "XL",
    "xxl" => "XXL",
};

#[derive(Debug, Clone)]
pub struct Model {
    pub id: String,
    pub parameters: Option<ModelParams>,
    pub quantization: Option<ModelQuant>,
    pub thinking: Option<ThinkingModes>,
}

#[derive(Debug, Clone)]
pub struct ThinkingModes<M = Vec<String>> {
    pub modes: M,
    pub budget: Option<ThinkingBudget>,
}

#[derive(Debug, Clone, Copy)]
pub struct ThinkingBudget {
    pub min: usize,
    pub max: usize,
}

impl Model {
    /// Returns a prettified model name.
    ///
    /// Strips the tag suffix after `:`, replaces `_` and `-` with spaces
    /// (except `-` between two digits becomes `.`), collapses consecutive
    /// spaces, and capitalizes each word. Words in [`REPLACEMENT_WORDS`] are
    /// fully uppercased.
    pub fn name(&self) -> String {
        let without_tag = self.id.split_once(':').map_or(&*self.id, |(name, _)| name);
        let base = without_tag
            .rfind(['/', '\\'])
            .map_or(without_tag, |pos| &without_tag[pos + 1..]);
        let chars: Vec<char> = base.chars().collect();
        let mut spaced = String::with_capacity(base.len());
        for (i, &c) in chars.iter().enumerate() {
            match c {
                '_' => spaced.push(' '),
                '-' => {
                    let prev_digit = i > 0 && chars[i - 1].is_ascii_digit();
                    let next_digit = i + 1 < chars.len() && chars[i + 1].is_ascii_digit();
                    if prev_digit && next_digit {
                        spaced.push('.');
                    } else {
                        spaced.push(' ');
                    }
                }
                _ => spaced.push(c),
            }
        }
        let mut result = String::new();
        for (i, word) in spaced.split_whitespace().enumerate() {
            if i > 0 {
                result.push(' ');
            }
            if let Some(&replacement) = REPLACEMENT_WORDS.get(&word.to_lowercase().as_str()) {
                result.push_str(replacement);
            } else {
                let mut chars = word.chars();
                if let Some(first) = chars.next() {
                    result.extend(first.to_uppercase());
                    result.extend(chars);
                }
            }
        }
        result
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelParams(String);

impl ModelParams {
    pub fn new(s: &str) -> Self {
        Self(s.to_uppercase())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelQuant(String);

impl ModelQuant {
    pub fn new(s: &str) -> Self {
        let uppered = s.to_uppercase();
        let mut result = String::with_capacity(uppered.len());
        let mut seen_sep = false;
        for c in uppered.chars() {
            if c == '_' || c == '-' {
                if !seen_sep {
                    result.push(':');
                }
                seen_sep = true;
            } else {
                result.push(c);
            }
        }
        Self(result)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl ToString for Model {
    fn to_string(&self) -> String {
        let name = self.name();
        match (&self.parameters, &self.quantization) {
            (Some(params), Some(quant)) => {
                format!("{} ({} {})", name, params.as_str(), quant.as_str())
            }
            (Some(params), None) => {
                format!("{} ({})", name, params.as_str())
            }
            (None, Some(quant)) => {
                format!("{} ({})", name, quant.as_str())
            }
            _ => name,
        }
    }
}
