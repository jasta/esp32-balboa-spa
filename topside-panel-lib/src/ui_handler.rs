use std::borrow::Borrow;
use embedded_graphics::draw_target::DrawTarget;
use lvgl::{Align, Color, LvResult, Part, State, UI, Widget};
use lvgl::style::Style;
use lvgl::widgets::Label;
use std::time::{Duration, Instant};
use std::thread;
use cstr_core::{CStr, CString};
use embedded_graphics::pixelcolor::{PixelColor};
use crate::topside_panel::{Button, ControlHandle, ViewModelEventHandle};

pub trait LcdDevice {
  type Display: DrawTarget;
  type Window: WindowProxy<Self::Display>;

  fn setup(self) -> (Self::Display, Self::Window);
}

pub struct UiHandler<DEV> {
  lcd_device: DEV,
  control_handle: ControlHandle,
  model_events: ViewModelEventHandle,
}

pub trait WindowProxy<D> {
  fn events(&mut self) -> Vec<UserInputEvent>;
  fn update(&mut self, display: &D);
}

pub enum UserInputEvent {
  Quit,
  ButtonPressed(Button),
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

    let mut screen = ui.scr_act()?;

    let mut screen_style = Style::default();
    screen_style.set_bg_color(State::DEFAULT, Color::from_rgb((0, 0, 0)));
    screen.add_style(Part::Main, screen_style)?;

    let mut label = Label::new(&mut screen)?;
    let mut label_style = Style::default();
    label_style.set_text_color(State::DEFAULT, Color::from_rgb((200, 200, 200)));
    label.add_style(Part::Main, label_style)?;
    label.set_align(&mut screen, Align::InLeftMid, 0, 0)?;
    label.set_text(CString::new("Hello...").unwrap().as_c_str());

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
        label.set_text(CString::new(format!("{:?}", model.conn_state)).unwrap().as_c_str());
      }

      thread::sleep(Duration::from_millis(20));
      ui.tick_inc(loop_started.elapsed());
      loop_started = Instant::now();
    }
  }
}
