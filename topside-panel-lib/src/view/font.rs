use lvgl_sys::lv_font_t;

pub enum Font {
  MONTSERRAT_12,
  MONTSERRAT_16,
  MONTSERRAT_24,
  MONTSERRAT_32,
  MONTSERRAT_48,
}

impl Font {
  pub fn raw(&self) -> *const lv_font_t {
    unsafe {
      let ptr = match self {
        Font::MONTSERRAT_12 => &lvgl_sys::lv_font_montserrat_12,
        Font::MONTSERRAT_16 => &lvgl_sys::lv_font_montserrat_16,
        Font::MONTSERRAT_24 => &lvgl_sys::lv_font_montserrat_24,
        Font::MONTSERRAT_32 => &lvgl_sys::lv_font_montserrat_32,
        Font::MONTSERRAT_48 => &lvgl_sys::lv_font_montserrat_48,
      };
      ptr as *const lv_font_t
    }
  }
}
