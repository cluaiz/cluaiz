use minijinja::{context, Environment};
use serde_json::json;

/// 🎭 TemplateManager: 100% Dynamic Metadata & Registry-Driven Jinja2 Prompt Formatter.
/// Resolves chat templates dynamically across 10,000+ models without hardcoding.
#[derive(Debug, Clone, Default)]
pub struct TemplateManager;

impl TemplateManager {
    /// Dynamically parses any input prompt into structured messages.
    fn parse_messages(prompt: &str) -> Vec<serde_json::Value> {
        if let Ok(msgs) = serde_json::from_str::<Vec<serde_json::Value>>(prompt) {
            msgs
        } else {
            vec![json!({ "role": "user", "content": prompt })]
        }
    }

    /// Pure dynamic MiniJinja renderer for any GGUF chat template.
    pub fn render_jinja(
        template_str: &str,
        messages: &[serde_json::Value],
        add_gen: bool,
    ) -> Result<String, minijinja::Error> {
        let mut env = Environment::new();
        env.add_function(
            "raise_exception",
            |err: String| -> Result<String, minijinja::Error> {
                Err(minijinja::Error::new(
                    minijinja::ErrorKind::InvalidOperation,
                    err,
                ))
            },
        );
        env.add_filter(
            "tojson",
            |val: minijinja::Value| -> Result<String, minijinja::Error> {
                serde_json::to_string(&val).map_err(|e| {
                    minijinja::Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string())
                })
            },
        );
        env.add_function("strftime_now", |format: Option<String>| -> String {
            let fmt = format.unwrap_or_else(|| "%Y-%m-%d".to_string());
            chrono::Utc::now().format(&fmt).to_string()
        });

        env.add_template("chat", template_str)?;
        let tmpl = env.get_template("chat")?;

        let ctx = context! {
            messages => messages,
            add_generation_prompt => add_gen,
            tools => Vec::<serde_json::Value>::new(),
            bos_token => "",
            eos_token => ""
        };

        tmpl.render(ctx)
    }

    /// Renders raw role/content message pairs using MiniJinja
    pub fn render_messages(
        template_str: &str,
        messages: &[(&str, &str)],
        add_gen: bool,
    ) -> Result<String, minijinja::Error> {
        let json_msgs: Vec<serde_json::Value> = messages
            .iter()
            .map(|(r, c)| serde_json::json!({ "role": *r, "content": *c }))
            .collect();
        Self::render_jinja(template_str, &json_msgs, add_gen)
    }

    /// 🌟 Dynamic Template Resolver:
    /// In-memory GGUF Binary Header (`tokenizer.chat_template`) -> Zero Disk I/O.
    fn resolve_template(dna: &crate::metadata::dna::StructuralDNA) -> Option<String> {
        if let Some(ref tmpl) = dna.chat_template {
            if !tmpl.trim().is_empty() {
                return Some(tmpl.clone());
            }
        }
        None
    }

    /// Dynamic format resolver: Pure metadata Jinja rendering with clean neutral fallbacks.
    pub fn format(&self, dna: &crate::metadata::dna::StructuralDNA, prompt: &str) -> String {
        let messages = Self::parse_messages(prompt);

        if let Some(template) = Self::resolve_template(dna) {
            if let Ok(rendered) = Self::render_jinja(&template, &messages, true) {
                return rendered;
            }
        }

        // Clean neutral fallback without injecting architecture-specific token tags
        let mut out = String::new();
        for m in &messages {
            let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
            out.push_str(&format!("{}: {}\n", role, content));
        }
        out.push_str("assistant:\n");
        out
    }

    /// Forms a strict mid-conversation turn for Pivot/Interrupt scenarios using dynamic template.
    pub fn format_turn(&self, dna: &crate::metadata::dna::StructuralDNA, prompt: &str) -> String {
        let single_turn_messages = vec![json!({ "role": "user", "content": prompt })];

        if let Some(template) = Self::resolve_template(dna) {
            if let Ok(rendered) = Self::render_jinja(&template, &single_turn_messages, true) {
                return rendered;
            }
        }

        format!("user: {}\nassistant:\n", prompt)
    }
}
