use cstr_core::CString;
use lvgl::{Align, Color, LvResult, NativeObject, Obj, Part, State, UI, Widget};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::PixelColor;
use log::warn;
use lvgl::widgets::{Arc, ArcPart, Label, Linemeter};
use crate::model::view_model::ViewModel;
use crate::view::palette::{Palette, PaletteAware};
use crate::view::palette_styles::PaletteStyles;
use crate::view::temperature_widget::TemperatureWidget;

pub(crate) const WIDGET_FG_STROKE_COLOR: u32 = 0xfffffff;
pub(crate) const LABEL_PRIMARY_COLOR: u32 = 0xffffff;

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
