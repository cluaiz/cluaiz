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
    let hw = cluaiz_hw::HardwareProber::current();
    let os = &hw.os_name;
    let arch = &hw.os_version;

    let cpu = &hw.cpu.brand;
    let cores = hw.cpu.physical_cores;
    let gpu_model = if let Some(gpu) = hw.gpus.first() {
        gpu.name.as_str()
    } else {
        "None"
    };
    let vram = hw.gpus.first().map(|g| g.vram_total_gb).unwrap_or(0.0);
    let ram = hw.memory.total_gb;

    let mode = "cluaiz Native";
    let opt = cluaiz_core::Settings::current().optimization;
    let flash = opt.flash_attention.is_active();
    let moe = opt.extreme_moe_streaming.is_active();

    let node_id = "LOCAL-NODE";
    let locale = "en_US";
    let timezone = "LOCAL";

    let dna_info = format!(
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
            OPTIMIZATION: FlashAttn: {} | MoE Streaming: {}\n\n\
            ── CONTROLS ─────────────────────\n\
            [ R ] RE-DETECT HARDWARE\n\
            [ ESC ] BACK TO MENU",
            node_id, os, arch, locale, timezone,
            cpu, cores,
            gpu_model, vram, ram,
            mode.to_uppercase(),
            if flash { "ON ⚡" } else { "OFF" },
            if moe { "ON ⚡" } else { "OFF" }
        );

    let widget = Paragraph::new(dna_info)
        .style(Style::default().fg(Color::White))
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" ⚙️  HARDWARE DNA ")
            .title_alignment(ratatui::layout::Alignment::Center));
    
    widget.render(area, buf);
}
