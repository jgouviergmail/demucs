use egui::{Color32, Visuals};
use demucs_core::model::metadata::StemId;

// ── Color palette (matches web/src/design/tokens.ts) ────────────────────────

pub const BG: Color32 = Color32::from_rgb(0x0b, 0x0a, 0x10);
pub const SURFACE: Color32 = Color32::from_rgb(0x14, 0x13, 0x1f);
pub const SURFACE2: Color32 = Color32::from_rgb(0x1e, 0x1d, 0x2e);
pub const BORDER: Color32 = Color32::from_rgb(0x2a, 0x29, 0x40);
pub const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe5, 0xf0);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x6e, 0x6b, 0x82);
pub const ACCENT_CORAL: Color32 = Color32::from_rgb(0xf0, 0x7a, 0x5c);
pub const ACCENT_PURPLE: Color32 = Color32::from_rgb(0x7c, 0x6f, 0xf0);
pub const SUCCESS: Color32 = Color32::from_rgb(0x6e, 0xe7, 0xa0);
pub const ERROR: Color32 = Color32::from_rgb(0xff, 0x64, 0x64);

// ── Stem colors ─────────────────────────────────────────────────────────────

pub fn stem_color(id: StemId) -> Color32 {
    match id {
        StemId::Drums => Color32::from_rgb(0xf0, 0xa0, 0x5c),
        StemId::Bass => Color32::from_rgb(0xb0, 0x7a, 0xf0),
        StemId::Vocals => Color32::from_rgb(0xf0, 0xd7, 0x5c),
        StemId::Other => Color32::from_rgb(0x6e, 0xe7, 0xa0),
        StemId::Guitar => Color32::from_rgb(0x5c, 0xb8, 0xf0),
        StemId::Piano => Color32::from_rgb(0xf0, 0x7a, 0x7a),
    }
}

// ── Magma colormap LUT (256 entries from 11 stops) ──────────────────────────

const MAGMA_STOPS: [(f32, u8, u8, u8); 11] = [
    (0.0, 0, 0, 4),
    (0.1, 20, 14, 54),
    (0.2, 59, 15, 112),
    (0.3, 100, 26, 128),
    (0.4, 140, 41, 129),
    (0.5, 183, 55, 121),
    (0.6, 222, 73, 104),
    (0.7, 247, 115, 92),
    (0.8, 254, 176, 120),
    (0.9, 253, 226, 163),
    (1.0, 252, 253, 191),
];

pub struct MagmaLut {
    pub entries: [[u8; 4]; 256],
}

impl MagmaLut {
    pub fn new() -> Self {
        let mut entries = [[0u8; 4]; 256];
        for i in 0..256 {
            let t = i as f32 / 255.0;
            // Find surrounding stops
            let mut lo = 0;
            for j in 0..MAGMA_STOPS.len() - 1 {
                if MAGMA_STOPS[j + 1].0 >= t {
                    lo = j;
                    break;
                }
            }
            let hi = lo + 1;
            let (t0, r0, g0, b0) = MAGMA_STOPS[lo];
            let (t1, r1, g1, b1) = MAGMA_STOPS[hi];
            let f = if (t1 - t0).abs() < 1e-6 {
                0.0
            } else {
                (t - t0) / (t1 - t0)
            };
            entries[i] = [
                lerp_u8(r0, r1, f),
                lerp_u8(g0, g1, f),
                lerp_u8(b0, b1, f),
                255,
            ];
        }
        Self { entries }
    }

    /// Look up a normalized value [0..1] in the LUT.
    pub fn lookup(&self, t: f32) -> [u8; 4] {
        let idx = (t.clamp(0.0, 1.0) * 255.0) as usize;
        self.entries[idx.min(255)]
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

// ── Apply theme to egui context ─────────────────────────────────────────────

pub fn apply_theme(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();

    visuals.panel_fill = BG;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = SURFACE;
    visuals.faint_bg_color = SURFACE2;

    // Widgets
    visuals.widgets.noninteractive.bg_fill = SURFACE;
    visuals.widgets.noninteractive.fg_stroke.color = TEXT_DIM;
    visuals.widgets.noninteractive.bg_stroke.color = BORDER;

    visuals.widgets.inactive.bg_fill = SURFACE2;
    visuals.widgets.inactive.fg_stroke.color = TEXT;
    visuals.widgets.inactive.bg_stroke.color = BORDER;

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0x28, 0x27, 0x3e);
    visuals.widgets.hovered.fg_stroke.color = TEXT;
    visuals.widgets.hovered.bg_stroke.color = ACCENT_PURPLE;

    visuals.widgets.active.bg_fill = ACCENT_PURPLE;
    visuals.widgets.active.fg_stroke.color = TEXT;

    visuals.widgets.open.bg_fill = SURFACE2;
    visuals.widgets.open.fg_stroke.color = TEXT;

    visuals.selection.bg_fill = Color32::from_rgba_premultiplied(0x7c, 0x6f, 0xf0, 60);
    visuals.selection.stroke.color = ACCENT_PURPLE;

    visuals.override_text_color = Some(TEXT);
    visuals.window_stroke.color = BORDER;

    ctx.set_visuals(visuals);
}
