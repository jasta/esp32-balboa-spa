use lvgl::style::{Opacity, Style};
use lvgl::State;
use crate::view::color_util;
use crate::view::palette::Palette;

pub struct PaletteStyles {
  pub window_bg: Style,
  pub widget_fill: Style,
  pub widget_bg_stroke: Style,
}

impl PaletteStyles {
  pub fn new(palette: Palette) -> Self {
    let mut window_bg = Style::default();
    window_bg.set_bg_color(State::DEFAULT, color_util::hex_color(palette.window_bg));

    let mut widget_fill = Style::default();
    widget_fill.set_bg_color(State::DEFAULT, color_util::hex_color(palette.widget_fill));

    let mut widget_bg_stroke = Style::default();
    widget_bg_stroke.set_line_color(State::DEFAULT, color_util::hex_color(palette.widget_bg_stroke));
    widget_bg_stroke.set_scale_grad_color(State::DEFAULT, color_util::hex_color(palette.widget_bg_stroke));
    widget_bg_stroke.set_scale_end_color(State::DEFAULT, color_util::hex_color(palette.widget_bg_stroke));

    Self {
      window_bg,
      widget_fill,
      widget_bg_stroke,
    }
  }
}
