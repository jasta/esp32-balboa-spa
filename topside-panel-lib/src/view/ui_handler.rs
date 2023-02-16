use std::borrow::Borrow;
use embedded_graphics::draw_target::DrawTarget;
use lvgl::{Align, Color, LvResult, Part, State, UI, Widget};
use lvgl::style::Style;
use lvgl::widgets::{Arc, Label};
use std::time::{Duration, Instant};
use std::thread;
use cstr_core::{CStr, CString};
use embedded_graphics::pixelcolor::PixelColor;
use common_lib::view_model_event_handle::ViewModelEventHandle;
use crate::view::main_screen::MainScreen;
use crate::network::topside_panel_client::ControlHandle;
use crate::view::lcd_device::{LcdDevice};
use crate::view::user_input_event::UserInputEvent;
use crate::view::window_proxy::WindowProxy;
use crate::model::view_model::ViewModel;
use crate::view::backlight_manager::BacklightManager;

/// Approximate time between each frame draw.
const TARGET_DRAW_INTERVAL: Duration = Duration::from_millis(20);

pub struct UiHandler<DEV> {
  lcd_device: DEV,
  control_handle: ControlHandle,
  app_events: ViewModelEventHandle<ViewModel>,
}

impl<DEV> UiHandler<DEV>
where
    DEV: LcdDevice,
    DEV::Display: DrawTarget,
    <<DEV as LcdDevice>::Display as DrawTarget>::Color: PixelColor + From<Color>,
{
  pub fn new(
      lcd_panel: DEV,
      control_handle: ControlHandle,
      app_events: ViewModelEventHandle<ViewModel>,
  ) -> Self {
    Self {
      lcd_device: lcd_panel,
      control_handle,
      app_events,
    }
  }

  pub fn run_loop(mut self) -> LvResult<()> {
    let (display, mut window, mut backlight) =
        self.lcd_device.setup();


    let mut ui = UI::init()?;
    ui.disp_drv_register(display)?;

    let mut main = MainScreen::setup(&ui)?;

    let event_update_interval = window.event_update_interval();
    assert!(event_update_interval <= TARGET_DRAW_INTERVAL);

    let mut last_tick = Instant::now();
    let mut backlight_manager = BacklightManager::init(backlight);
    loop {
      ui.task_handler();

      backlight_manager.detect_inactivity(last_tick);

      window.update(ui.get_display_ref().unwrap());

      'event_handler: loop {
        for event in window.events() {
          match event {
            UserInputEvent::Quit => {
              self.control_handle.request_shutdown();
              return Ok(());
            }
            UserInputEvent::ButtonPressed(b) => {
              self.control_handle.send_button_pressed(b);
              backlight_manager.mark_user_activity(Instant::now());
            }
          }
        }

        thread::sleep(event_update_interval);

        if last_tick.elapsed() >= TARGET_DRAW_INTERVAL {
          break 'event_handler;
        }
      }

      if let Some(model) = self.app_events.try_recv_latest().unwrap() {
        main.bind(model)?;
      }

      let now = Instant::now();
      ui.tick_inc(now - last_tick);
      last_tick = now;
    }
  }
}