use cstr_core::CString;
use lvgl::{Align, Color, LvResult, NativeObject, Obj, Part, State, UI, Widget};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::PixelColor;
use log::warn;
use lvgl::widgets::{Arc, ArcPart, Label, Linemeter};
use wifi_module_lib::view_model::Mode;
use crate::model::view_model::{HotTubModel, ViewModel};
use crate::view::palette::{Palette, PaletteAware};
use crate::view::palette_styles::PaletteStyles;
use crate::view::screen_flipper::{BoxedScreen, Screen, ScreenSelector};
use crate::view::temperature_widget::TemperatureWidget;

pub(crate) const WIDGET_FG_STROKE_COLOR: u32 = 0xfffffff;
pub(crate) const LABEL_PRIMARY_COLOR: u32 = 0xffffff;

pub(crate) const NORMAL: Palette = Palette {
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
  pub fn new() -> LvResult<Self> {
    let mut screen = Obj::default();

    let styles = Styles::new();
    let temperature_widget = TemperatureWidget::new(&mut screen)?;

    Ok(Self {
      screen,
      styles,
      temperature_widget,
      is_heating_palette: None,
    })
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

  fn get_hot_tub_model(model: &ViewModel) -> Option<&HotTubModel> {
    model.last_model.as_ref()
  }
}

impl ScreenSelector for MainScreen {
  fn kind() -> &'static str {
    "main"
  }

  fn create() -> LvResult<BoxedScreen> {
    Ok(Box::new(MainScreen::new()?))
  }

  fn accept_model(model: &ViewModel) -> bool {
    if MainScreen::get_hot_tub_model(model).is_none() {
      return false;
    }

    // Stick with the loading screen until Wi-Fi at least initializes in case we
    // are supposed to go the provisioning screen.
    match &model.wifi_model {
      None => true,
      Some(wifi_model) => {
        !matches!(wifi_model.mode, Mode::Initializing)
      }
    }
  }
}

impl Screen for MainScreen {
  fn get_root(&self) -> &Obj {
    &self.screen
  }

  fn bind_model(&mut self, model: ViewModel) -> LvResult<()> {
    let model = MainScreen::get_hot_tub_model(&model).unwrap();
    self.set_is_heating(model.is_heating)?;
    let range = model.temp_range.display;
    self.temperature_widget.set_range(&range.0, &range.1)?;
    self.temperature_widget.set_target(&model.set_temp.display)?;
    self.temperature_widget.set_current(
        model.current_temp.as_ref().map(|t| &t.display))?;
    let action_label = if model.is_heating { "HEATING" } else { "" };
    self.temperature_widget.set_action_text(action_label)?;
    Ok(())
  }
}
