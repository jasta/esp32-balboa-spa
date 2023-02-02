use cstr_core::{CString};
use lvgl::{Align, Color, LvResult, NativeObject, Obj, Part, State, UI, Widget};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::PixelColor;
use log::warn;
use lvgl::style::Style;
use lvgl::widgets::{Arc, ArcPart, Label, Linemeter};
use lvgl_sys::{lv_font_t, lv_obj_refresh_style};
use crate::temperature_model::TemperatureDisplay;
use crate::view_model::ViewModel;

const WIDGET_FG_STROKE_COLOR: u32 = 0xfffffff;
const LABEL_PRIMARY_COLOR: u32 = 0xffffff;

const NORMAL: Palette = Palette {
  window_bg: 0x393f47,
  widget_fill: 0x3d444b,
  widget_bg_stroke: 0x434a52,
};

const HEATING: Palette = Palette {
  window_bg: 0xdb742c,
  widget_fill: 0xdd7e2f,
  widget_bg_stroke: 0xdf8631,
};

pub struct MainScreen {
  screen: Obj,
  styles: Styles,
  temperature_widget: TemperatureWidget,
  is_heating_palette: Option<bool>,
}

struct Styles {
  normal: PaletteStyles,
  heating: PaletteStyles,
}

struct TemperatureWidget {
  linemeter: Linemeter,
  main_label: TemperatureLabel,
  action_label: Label,
}

impl TemperatureWidget {
  pub fn new(parent: &mut impl NativeObject) -> LvResult<Self> {
    let mut linemeter_style = Style::default();
    linemeter_style.set_border_width(State::DEFAULT, 0);
    linemeter_style.set_line_width(State::DEFAULT, 1);
    linemeter_style.set_scale_end_line_width(State::DEFAULT, 1);

    let padding = 5;
    linemeter_style.set_pad_left(State::DEFAULT, padding);
    linemeter_style.set_pad_top(State::DEFAULT, padding);
    linemeter_style.set_pad_right(State::DEFAULT, padding);

    let mut linemeter = Linemeter::new(parent)?;
    linemeter.add_style(Part::Main, linemeter_style.clone())?;
    linemeter.set_size(200, 200)?;
    linemeter.set_scale(280, 100)?;
    linemeter.set_align(parent, Align::Center, 0, 0)?;

    let mut main_label = TemperatureLabel::new(
        &mut linemeter,
        Font::MONTSERRAT_48,
        Font::MONTSERRAT_24)?;

    let mut action_style = Style::default();
    action_style.set_text_color(State::DEFAULT, hex_color(LABEL_PRIMARY_COLOR));
    style_set_text_font(&mut action_style, State::DEFAULT, Font::MONTSERRAT_12);
    let mut action_label = Label::new(&mut linemeter)?;
    action_label.add_style(Part::Main, action_style.clone())?;
    action_label.set_align(&mut main_label.large_label, Align::OutTopMid, 0, 0)?;
    obj_set_auto_realign(&mut action_label, true)?;

    Ok(Self {
      linemeter,
      main_label,
      action_label,
    })
  }

  pub fn set_range(&mut self, scale: (TemperatureDisplay, TemperatureDisplay)) -> LvResult<()> {
    self.linemeter.set_range(scale.0.int_value, scale.1.int_value)
  }

  pub fn set_target(&mut self, value: TemperatureDisplay) -> LvResult<()> {
    self.linemeter.set_value(value.int_value)?;
    self.main_label.set_temperature(value)?;
    Ok(())
  }

  pub fn set_action_text(&mut self, value: &str) -> LvResult<()> {
    self.action_label.set_text(CString::new(value).unwrap().as_c_str())
  }

  pub fn set_current(&mut self, value: TemperatureDisplay) -> LvResult<()> {
    todo!()
  }
}

impl PaletteAware for TemperatureWidget {
  fn apply(&self, styles: &PaletteStyles) -> LvResult<()> {
    self.linemeter.add_style(Part::Main, styles.widget_fill.clone())?;
    self.linemeter.add_style(Part::Main, styles.widget_bg_stroke.clone())?;
    Ok(())
  }
}

struct TemperatureLabel {
  large_label: Label,
  small_label: Label,
}

impl TemperatureLabel {
  pub fn new(parent: &mut impl NativeObject, large_font: Font, small_font: Font) -> LvResult<Self> {
    let mut large_style = Style::default();
    large_style.set_text_color(State::DEFAULT, hex_color(LABEL_PRIMARY_COLOR));
    style_set_text_font(&mut large_style, State::DEFAULT, large_font);
    let mut small_style = Style::default();
    small_style.set_text_color(State::DEFAULT, hex_color(LABEL_PRIMARY_COLOR));
    style_set_text_font(&mut large_style, State::DEFAULT, small_font);

    let mut large_label = Label::new(parent)?;
    large_label.add_style(Part::Main, large_style.clone())?;
    large_label.set_align(parent, Align::Center, 0, 0)?;
    obj_set_auto_realign(&mut large_label, true)?;

    let mut small_label = Label::new(parent)?;
    small_label.add_style(Part::Main, small_style.clone())?;
    small_label.set_align(&mut large_label, Align::OutRightTop, 0, 0)?;
    obj_set_auto_realign(&mut small_label, true)?;
    Ok(Self {
      large_label,
      small_label,
    })
  }

  pub fn set_temperature(&mut self, display: TemperatureDisplay) -> LvResult<()> {
    self.large_label.set_text(
      CString::new(display.big_part.to_string()).unwrap().as_c_str())?;

    let little_part = display.little_part.map(|v| {
      v.to_string()
    }).unwrap_or_else(|| "".to_owned());
    self.small_label.set_text(CString::new(little_part).unwrap().as_c_str())?;

    Ok(())
  }
}

impl Styles {
  pub fn new() -> Self {
    Self {
      normal: PaletteStyles::new(NORMAL),
      heating: PaletteStyles::new(HEATING),
    }
  }

  pub fn select_palette(&self, is_heating: bool) -> &PaletteStyles {
    match is_heating {
      true => &self.heating,
      false => &self.normal,
    }
  }
}

struct PaletteStyles {
  window_bg: Style,
  widget_fill: Style,
  widget_bg_stroke: Style,
}

impl PaletteStyles {
  pub fn new(palette: Palette) -> Self {
    let mut window_bg = Style::default();
    window_bg.set_bg_color(State::DEFAULT, hex_color(palette.window_bg));

    let mut widget_fill = Style::default();
    widget_fill.set_bg_color(State::DEFAULT, hex_color(palette.widget_fill));

    let mut widget_bg_stroke = Style::default();
    widget_bg_stroke.set_line_color(State::DEFAULT, hex_color(palette.widget_bg_stroke));
    widget_bg_stroke.set_scale_grad_color(State::DEFAULT, hex_color(palette.widget_bg_stroke));
    widget_bg_stroke.set_scale_end_color(State::DEFAULT, hex_color(palette.widget_bg_stroke));

    Self {
      window_bg,
      widget_fill,
      widget_bg_stroke,
    }
  }
}

trait PaletteAware {
  fn apply(&self, styles: &PaletteStyles) -> LvResult<()>;
}

impl MainScreen {
  pub fn setup<T, C>(ui: &UI<T, C>) -> LvResult<Self>
  where
      T: DrawTarget<Color = C>,
      C: PixelColor + From<Color>,
  {
    let mut screen = ui.scr_act()?;

    let styles = Styles::new();
    let temperature_widget = TemperatureWidget::new(&mut screen)?;

    Ok(Self {
      screen,
      styles,
      temperature_widget,
      is_heating_palette: None,
    })
  }

  pub fn bind(&mut self, model: ViewModel) -> LvResult<()> {
    if let Some(model) = model.last_model {
      self.set_is_heating(model.is_heating)?;
      self.temperature_widget.set_range(model.temp_range.display)?;
      self.temperature_widget.set_target(model.set_temp.display)?;
      let action_label = if model.is_heating { "HEATING" } else { "" };
      self.temperature_widget.set_action_text(action_label)?;
    } else {
      warn!("Initializing states not implemented...");
      self.set_is_heating(false)?;
    }
    Ok(())
  }

  fn set_is_heating(&mut self, is_heating: bool) -> LvResult<()> {
    if self.is_heating_palette != Some(is_heating) {
      self.is_heating_palette = Some(is_heating);

      let palette = self.styles.select_palette(is_heating);

      self.screen.add_style(Part::Main, palette.window_bg.clone())?;
      self.temperature_widget.apply(palette)?;
    }
    Ok(())
  }
}

fn hex_color(c: u32) -> Color {
  let b = c & 0xff;
  let g = (c >> 8) & 0xff;
  let r = (c >> 16) & 0xff;
  Color::from_rgb((r as u8, g as u8, b as u8))
}

fn style_set_text_font(style: &mut Style, state: State, value: Font) {
  let native_state: u32 = state.get_bits();
  unsafe {
    lvgl_sys::_lv_style_set_ptr(
      style.raw(),
      (lvgl_sys::LV_STYLE_TEXT_FONT as u32
          | (native_state << lvgl_sys::LV_STYLE_STATE_POS as u32)) as u16,
      value.raw() as *mut cty::c_void,
    );
  }
}

fn obj_set_auto_realign<C>(obj: &mut C, value: bool) -> LvResult<()>
where
    C: NativeObject,
{
  unsafe {
    lvgl_sys::lv_obj_set_auto_realign(
      obj.raw()?.as_mut(),
      value);
  }
  Ok(())
}

pub enum Font {
  MONTSERRAT_12,
  MONTSERRAT_16,
  MONTSERRAT_24,
  MONTSERRAT_32,
  MONTSERRAT_48,
}

impl Font {
  fn raw(&self) -> *const lv_font_t {
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

struct Palette {
  /// Color that the whole window is painted
  window_bg: u32,

  /// Color that the majority of the widget background is painted
  widget_fill: u32,

  /// Muted stroke color that non-foreground strokes use
  widget_bg_stroke: u32,
}
