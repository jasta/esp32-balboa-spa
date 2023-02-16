use cstr_core::CString;
use lvgl::{Align, LvResult, Obj, Part, State, Widget};
use lvgl::style::Style;
use lvgl::widgets::Label;
use crate::model::view_model::ViewModel;
use crate::view::color_util::hex_color;
use crate::view::font::Font;
use crate::view::lvgl_ext::{obj_set_auto_realign, style_set_text_font};
use crate::view::main_screen;
use crate::view::main_screen::LABEL_PRIMARY_COLOR;
use crate::view::palette_styles::PaletteStyles;
use crate::view::screen_flipper::{BoxedScreen, Screen, ScreenSelector};

pub struct LoadingScreen {
  screen: Obj,
  styles: Styles,
  label: Label,
}

struct Styles {
  normal: PaletteStyles,
}

impl Styles {
  pub fn new() -> Self {
    Self {
      normal: PaletteStyles::new(main_screen::NORMAL),
    }
  }
}

impl LoadingScreen {
  pub fn new() -> LvResult<Self> {
    let mut screen = Obj::default();
    let styles = Styles::new();

    screen.add_style(Part::Main, styles.normal.window_bg.clone())?;

    let mut style = Style::default();
    style.set_text_color(State::DEFAULT, hex_color(LABEL_PRIMARY_COLOR));
    style_set_text_font(&mut style, State::DEFAULT, Font::MONTSERRAT_12);
    let mut label = Label::new(&mut screen)?;
    label.add_style(Part::Main, style.clone())?;
    label.set_align(&mut screen, Align::InBottomLeft, 10, 10)?;
    obj_set_auto_realign(&mut label, true)?;

    label.set_text(CString::new("Loading...").unwrap().as_c_str())?;

    Ok(Self {
      screen,
      styles,
      label,
    })
  }
}

impl ScreenSelector for LoadingScreen {
  fn kind() -> &'static str {
    "loading"
  }

  fn create() -> LvResult<BoxedScreen> {
    Ok(Box::new(LoadingScreen::new()?))
  }

  fn accept_model(model: &ViewModel) -> bool {
    true
  }
}

impl Screen for LoadingScreen {
  fn get_root(&self) -> &Obj {
    &self.screen
  }

  fn bind_model(&mut self, model: ViewModel) -> LvResult<()> {
    Ok(())
  }
}
