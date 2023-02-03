use std::borrow::Borrow;
use embedded_graphics::draw_target::DrawTarget;
use lvgl::{Align, Color, LvResult, Part, State, UI, Widget};
use lvgl::style::Style;
use lvgl::widgets::{Arc, Label};
use std::time::{Duration, Instant};
use std::thread;
use cstr_core::{CStr, CString};
use embedded_graphics::pixelcolor::PixelColor;
use crate::view::main_screen::MainScreen;
use crate::network::topside_panel::ControlHandle;
use crate::view::lcd_device::LcdDevice;
use crate::view::user_input_event::UserInputEvent;
use crate::view::window_proxy::WindowProxy;
use crate::model::view_model_event_handle::ViewModelEventHandle;
use crate::model::button::Button;

pub struct UiHandler<DEV> {
  lcd_device: DEV,
  control_handle: ControlHandle,
  model_events: ViewModelEventHandle,
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
      app_events: ViewModelEventHandle,
  ) -> Self {
    Self {
      lcd_device: lcd_panel,
      control_handle,
      model_events: app_events,
    }
  }

  pub fn run_loop(mut self) -> LvResult<()> {
    let (display, mut window) = self.lcd_device.setup();

    let mut ui = UI::init()?;
    ui.disp_drv_register(display)?;

    let mut main = MainScreen::setup(&ui)?;

    let mut loop_started = Instant::now();
    loop {
      ui.task_handler();

      window.update(ui.get_display_ref().unwrap());

      for event in window.events() {
        match event {
          UserInputEvent::Quit => {
            self.control_handle.request_shutdown();
            return Ok(());
          }
          UserInputEvent::ButtonPressed(b) => {
            self.control_handle.send_button_pressed(b);
          }
        }
      }

      if let Some(model) = self.model_events.try_recv_latest().unwrap() {
        main.bind(model)?;
      }

      thread::sleep(Duration::from_millis(20));
      ui.tick_inc(loop_started.elapsed());
      loop_started = Instant::now();
    }
  }
}
