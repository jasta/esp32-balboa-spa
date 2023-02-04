use lvgl::{Align, LvResult, NativeObject, Part, State, Widget};
use lvgl::style::Style;
use lvgl::widgets::{Label, Linemeter};
use cstr_core::CString;
use log::info;
use crate::model::temperature_model::TemperatureDisplay;
use crate::view::color_util::hex_color;
use crate::view::font::Font;
use crate::view::lvgl_ext::{obj_set_auto_realign, style_set_text_font};
use crate::view::main_screen::LABEL_PRIMARY_COLOR;
use crate::view::palette::PaletteAware;
use crate::view::palette_styles::PaletteStyles;

pub struct TemperatureWidget {
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
  current_value: Option<TemperatureDisplay>,
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
      current_value: None,
      large_label,
      small_label,
    })
  }

  pub fn set_temperature(&mut self, display: TemperatureDisplay) -> LvResult<()> {
    if self.current_value != Some(display) {
      self.current_value = Some(display);

      info!("UI temp: {display:?}");

      self.large_label.set_text(
        CString::new(display.big_part.to_string()).unwrap().as_c_str())?;

      let little_part = display.little_part.map(|v| {
        v.to_string()
      }).unwrap_or_else(|| "".to_owned());
      self.small_label.set_text(CString::new(little_part).unwrap().as_c_str())?;
    }
    Ok(())
  }
}
