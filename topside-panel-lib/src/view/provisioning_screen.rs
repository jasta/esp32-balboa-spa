use log::{info, warn};
use lvgl::{LvResult, Obj, Part, State, Widget};
use lvgl::style::Style;
use color_util::hex_color;
use wifi_module_lib::view_model::{Mode, UnprovisionedModel};
use crate::model::view_model::ViewModel;
use crate::view::{color_util};
use crate::view::qr_code_widget::{QrCodeWidget, SetFromSourceError};
use crate::view::qr_code_widget::Source::Text;
use crate::view::screen_flipper::{BoxedScreen, Screen, ScreenOptions, ScreenSelector};

pub(crate) const LABEL_PRIMARY_COLOR: u32 = 0x000000;

pub struct ProvisioningScreen {
  screen: Obj,
  styles: Styles,
  qr_widget: QrCodeWidget,
}

struct Styles {
  window_bg: Style,
}

impl Styles {
  pub fn new() -> Self {
    let mut window_bg = Style::default();
    window_bg.set_bg_color(State::DEFAULT, hex_color(0xffffff));

    Self {
      window_bg,
    }
  }
}

impl ProvisioningScreen {
  pub fn new() -> LvResult<Self> {
    let mut screen = Obj::default();
    let styles = Styles::new();

    screen.add_style(Part::Main, styles.window_bg.clone())?;

    let qr_widget = QrCodeWidget::new(&mut screen)?;

    Ok(Self {
      screen,
      styles,
      qr_widget,
    })
  }

  fn get_unprovisioned_model(model: &ViewModel) -> Option<&UnprovisionedModel> {
    if let Some(wifi) = &model.wifi_model {
      if let Mode::NeedsProvisioning(unprovisioned) = &wifi.mode {
        return Some(unprovisioned);
      }
    }
    None
  }
}

impl ScreenSelector for ProvisioningScreen {
  fn kind() -> &'static str {
    "provisioning"
  }

  fn create() -> LvResult<BoxedScreen> {
    Ok(Box::new(ProvisioningScreen::new()?))
  }

  fn accept_model(model: &ViewModel) -> bool {
    ProvisioningScreen::get_unprovisioned_model(model).is_some()
  }
}

impl Screen for ProvisioningScreen {
  fn options(&self) -> ScreenOptions {
    ScreenOptions {
      force_backlight: true,
    }
  }

  fn get_root(&self) -> &Obj {
    &self.screen
  }

  fn bind_model(&mut self, model: ViewModel) -> LvResult<()> {
    let unprovisioned =
        ProvisioningScreen::get_unprovisioned_model(&model).unwrap();
    let code = &unprovisioned.params.dpp_qr_code;
    info!("Generating QR code from: {code}");

    let help_text = match self.qr_widget.set_qr_code_from_src(Some(Text(code.clone()))) {
      Ok(_) => "Scan the above code in your phone's Wi-Fi settings",
      Err(SetFromSourceError::EncodeError(e)) => {
        warn!("QR code encode failed: {e:?}");
        "QR code error!"
      }
      Err(SetFromSourceError::LvglError(e)) => return Err(e),
    };
    self.qr_widget.set_help_text(help_text)?;
    Ok(())
  }
}
