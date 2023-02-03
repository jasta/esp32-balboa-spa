use lvgl::Color;

pub fn hex_color(c: u32) -> Color {
  let b = c & 0xff;
  let g = (c >> 8) & 0xff;
  let r = (c >> 16) & 0xff;
  Color::from_rgb((r as u8, g as u8, b as u8))
}
