use cstr_core::CString;
use lvgl::{Align, Color, LvError, LvResult, NativeObject, Part, State, Widget};
use lvgl::style::{Opacity, Style};
use lvgl::widgets::{Canvas, Label, LabelAlign};
use lvgl_sys::{lv_color_int_t, lv_coord_t, lv_obj_get_width};
use qrcodegen::{DataTooLong, QrCode, QrCodeEcc, QrSegment};

use crate::view::color_util::hex_color;
use crate::view::font::Font;
use crate::view::lvgl_ext::{canvas_fill_bg, canvas_get_img, canvas_set_palette, canvas_set_px, canvas_set_size, color_from_full, ImgColorFormat, label_set_long_mode, LabelLongMode, obj_get_height, obj_get_width, obj_set_auto_realign, style_set_text_font};
use crate::view::{main_screen, provisioning_screen};

pub struct QrCodeWidget {
  canvas: Canvas,
  help_label: Label,
  canvas_size: lv_coord_t,
  light_color: Color,
  dark_color: Color,
  last_code_src: Option<Source>,
}

impl QrCodeWidget {
  pub fn new(parent: &mut impl NativeObject) -> LvResult<Self> {
    let mut canvas = Canvas::new(parent)?;
    canvas.set_align(parent, Align::Center, 0, -6)?;
    obj_set_auto_realign(&mut canvas, true)?;

    let mut help_style = Style::default();
    help_style.set_text_color(State::DEFAULT, hex_color(provisioning_screen::LABEL_PRIMARY_COLOR));
    style_set_text_font(&mut help_style, State::DEFAULT, Font::MONTSERRAT_12);

    let mut help_label = Label::new(parent)?;
    label_set_long_mode(&mut help_label, LabelLongMode::Break)?;
    help_label.set_width(320)?;
    help_label.set_label_align(LabelAlign::Center)?;
    help_label.add_style(Part::Main, help_style.clone())?;
    obj_set_auto_realign(&mut help_label, true)?;

    let mut me = Self {
      canvas_size: 0,
      canvas,
      help_label,
      light_color: Color::from_raw(unsafe { lvgl_sys::_LV_COLOR_TRANSP() }),
      dark_color: hex_color(0x000000),
      last_code_src: None,
    };

    me.set_size(200)?;
    me.help_label.set_align(&mut me.canvas, Align::OutBottomMid, 0, 2)?;

    Ok(me)
  }

  pub fn set_help_text(&mut self, value: &str) -> LvResult<()> {
    self.help_label.set_text(CString::new(value).unwrap().as_c_str())
  }

  pub fn set_size(&mut self, size: lv_coord_t) -> LvResult<()> {
    if self.canvas_size != size {
      self.canvas_size = size;
      let (width, height) = (size, size);
      let canvas = &mut self.canvas;
      canvas_set_size(canvas, width, height, ImgColorFormat::Indexed1Bit)?;
      canvas_set_palette(canvas, PaletteColor::Light.to_index(), self.light_color.clone())?;
      canvas_set_palette(canvas, PaletteColor::Dark.to_index(), self.dark_color.clone())?;
    }
    Ok(())
  }

  pub fn set_qr_code_from_src(&mut self, src: Option<Source>) -> Result<(), SetFromSourceError> {
    if self.last_code_src.as_ref() != src.as_ref() {
      let encoded = if let Some(src) = src.as_ref() {
        let segments = match src {
          Source::Text(data) => QrSegment::make_segments(data),
          Source::Binary(data) => vec![QrSegment::make_bytes(data)],
        };
        Some(QrCode::encode_segments(&segments, QrCodeEcc::Medium)
            .map_err(SetFromSourceError::EncodeError)?)
      } else { None };

      self.last_code_src = src;

      self.set_encoded_qr_code(encoded.as_ref())
          .map_err(SetFromSourceError::LvglError)
    } else {
      Ok(())
    }
  }

  pub fn set_encoded_qr_code(&mut self, code: Option<&QrCode>) -> LvResult<()> {
    canvas_fill_bg(&mut self.canvas, PaletteColor::default().into(), Opacity::OPA_COVER)?;
    if let Some(code) = code {
      self.set_qr_code_internal(code)?;
    }
    Ok(())
  }

  #[allow(clippy::useless_conversion)]
  fn set_qr_code_internal(&mut self, code: &QrCode) -> LvResult<()> {
    let qr_size = lv_coord_t::from(i16::try_from(code.size()).unwrap());

    let canvas_size = self.canvas_size;
    let scale = canvas_size / qr_size;
    let scaled = qr_size * scale;
    let margin = (canvas_size - scaled) / 2;

    let light_color = Self::make_color_pair(PaletteColor::Light);
    let dark_color = Self::make_color_pair(PaletteColor::Dark);
    let default_color = PaletteColor::default();

    let canvas = &mut self.canvas;
    for canvas_y in margin..scaled+margin {
      for canvas_x in margin..scaled+margin {
        let code_x = (canvas_x - margin) / scale;
        let code_y = (canvas_y - margin) / scale;
        let color = match code.get_module(code_x.into(), code_y.into()) {
          true => &dark_color,
          false => &light_color,
        };
        if color.1 != default_color {
          canvas_set_px(canvas, canvas_x, canvas_y, &color.0)?;
        }
      }
    }

    Ok(())
  }

  fn make_color_pair(palette_color: PaletteColor) -> (Color, PaletteColor) {
    (palette_color.into(), palette_color)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PaletteColor {
  Light = 0,
  Dark = 1,
}

impl PaletteColor {
  pub fn to_index(&self) -> u8 {
    *self as u8
  }
}

impl Default for PaletteColor {
  fn default() -> Self {
    Self::Light
  }
}

impl From<PaletteColor> for Color {
  fn from(value: PaletteColor) -> Self {
    color_from_full(lv_color_int_t::from(value.to_index()))
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
  Text(String),
  Binary(Vec<u8>),
}

pub enum SetFromSourceError {
  EncodeError(DataTooLong),
  LvglError(LvError),
}
