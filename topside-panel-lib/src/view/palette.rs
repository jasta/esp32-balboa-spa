use lvgl::LvResult;
use crate::view::palette_styles::PaletteStyles;

pub trait PaletteAware {
  fn apply(&self, styles: &PaletteStyles) -> LvResult<()>;
}

pub struct Palette {
  /// Color that the whole window is painted
  pub window_bg: u32,

  /// Color that the majority of the widget background is painted
  pub widget_fill: u32,

  /// Muted stroke color that non-foreground strokes use
  pub widget_bg_stroke: u32,
}
