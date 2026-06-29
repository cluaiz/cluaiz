use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Widget},
    buffer::Buffer,
};
use crate::core::state::AppState;
use crate::theme::Theme;

/// Hardware DNA Screen: Displays specs and Identity Card
pub fn render_widget(_app: &mut AppState, _theme: &Theme, area: Rect, buf: &mut Buffer) {
    let dna_info = if let Ok(raw) = std::fs::read_to_string("src/hardware/system_control.json") {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            let os = v["system_identity"]["os_target"].as_str().unwrap_or("?");
            let arch = v["system_identity"]["architecture"].as_str().unwrap_or("?");

            let cpu = v["cpu_brand"].as_str().unwrap_or("?");
            let cores = v["cpu_cores"].as_u64().unwrap_or(0);
            let gpu_model = if v["compute"]["has_gpu"].as_bool().unwrap_or(false) { "Accelerator Active" } else { "None" };
            let vram = v["compute"]["vram_gb"].as_f64().unwrap_or(0.0);
            let ram = v["memory"]["total_ram_gb"].as_f64().unwrap_or(0.0);

            let mode = "cluaiz Native";
            let flash = v["runtime_engine"]["booster_flags"]["FlashAttention_v2"].as_bool().unwrap_or(false);
            let turbo = v["runtime_engine"]["booster_flags"]["TurboQuant_Enable"].as_bool().unwrap_or(false);


            let ctx = v["system_context"].clone();
            let node_id = ctx["machine_id"].as_str().unwrap_or("UNKNOWN");
            let locale = ctx["system_locale"].as_str().unwrap_or("?");
            let timezone = ctx["timezone"].as_str().unwrap_or("?");

            format!(
                " 🧬 cluaiz IDENTITY CARD \n\n\
                NODE-ID:      {}\n\
                OS:           {} ({})\n\
                LOCALE:       {} | TZ: {}\n\n\
                ── HARDWARE SPECS ───────────────\n\
                CPU:          {} ({} cores)\n\
                GPU:          {} ({:.1}GB VRAM)\n\
                RAM:          {:.1} GB\n\n\
                ── ENGINE DNA ───────────────────\n\
                RUN MODE:     {}\n\
                BOOST:        FlashAttn: {} | TurboQuant: {}\n\n\
                ── CONTROLS ─────────────────────\n\
                [ R ] RE-DETECT HARDWARE\n\
                [ ESC ] BACK TO MENU",
                node_id, os, arch, locale, timezone,
                cpu, cores,
                gpu_model, vram, ram,
                mode.to_uppercase(),
                if flash { "ON ⚡" } else { "OFF" },
                if turbo { "ON ⚡" } else { "OFF" }
            )
        } else {
            "❌ system_control.json parse error".to_string()
        }
    } else {
        "❌ system_control.json missing".to_string()
    };

    let widget = Paragraph::new(dna_info)
        .style(Style::default().fg(Color::White))
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" ⚙️  HARDWARE DNA ")
            .title_alignment(ratatui::layout::Alignment::Center));
    
    widget.render(area, buf);
}
