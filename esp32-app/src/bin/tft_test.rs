use std::marker::PhantomData;
use std::thread;
use std::time::{Duration, Instant};
use anyhow::anyhow;
use display_interface_spi::{SPIInterface, SPIInterfaceNoCS};
use embedded_graphics::pixelcolor::{BinaryColor, Rgb565, Rgb666, Rgb888};
use embedded_graphics::pixelcolor::raw::{RawU16, RawU18};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle, Triangle};
use embedded_hal::spi::{MODE_0, MODE_3};
use esp_idf_hal::delay::Ets;
use esp_idf_hal::gpio::{Gpio0, PinDriver};
use esp_idf_hal::prelude::*;
use esp_idf_hal::spi;
use esp_idf_hal::spi::{Dma, SpiDeviceDriver};
use esp_idf_hal::spi::config::V02Type;
use log::{debug, info};
use lvgl::style::{Opacity, Style};
use lvgl::{Align, Color, LvResult, Part, State, UI, Widget};
use lvgl::widgets::Gauge;
use mipidsi::{Builder, Orientation};

fn main() -> anyhow::Result<()> {
  esp_idf_sys::link_patches();

  esp_idf_svc::log::EspLogger::initialize_default();

  let peripherals = Peripherals::take()
      .ok_or_else(|| anyhow!("Unable to take peripherals"))?;

  let spi = peripherals.spi2;

  // ESP32-C3
  let rst = PinDriver::output(peripherals.pins.gpio3)?;
  let dc_rs = PinDriver::output(peripherals.pins.gpio4)?;
  let mut backlight = PinDriver::output(peripherals.pins.gpio5)?;
  let sclk = peripherals.pins.gpio6;
  let sdo = peripherals.pins.gpio7;
  let sdi = peripherals.pins.gpio8;
  let mut cs = PinDriver::output(peripherals.pins.gpio10)?;

  // ESP32
  // let mut cs = PinDriver::output(peripherals.pins.gpio15)?;
  // let rst = PinDriver::output(peripherals.pins.gpio32)?;
  // let dc_rs = PinDriver::output(peripherals.pins.gpio33)?;
  // let sdi = peripherals.pins.gpio19;
  // let sclk = peripherals.pins.gpio18;
  // let mut backlight = PinDriver::output(peripherals.pins.gpio25)?;
  // let sdo = peripherals.pins.gpio23;

  info!("Disabling Chip Select...");
  cs.set_high()?;

  info!("Setting backlight low...");
  backlight.set_low()?;

  let mut delay = Ets;

  let config = spi::config::Config::new()
      .baudrate(25.MHz().into())
      .data_mode(V02Type(MODE_0).into());

  info!("Initializing SPI device...");
  let device =
      SpiDeviceDriver::new_single(spi, sclk, sdo, Option::<Gpio0>::None, Dma::Disabled, Option::<Gpio0>::None, &config)?;
  // display interface abstraction from SPI and DC
  let di = SPIInterface::new(device, dc_rs, cs);

  info!("Initializing driver...");
  // create driver
  // let mut display = Builder::ili9486_rgb565(di)
  //     .init(&mut delay, Some(rst))
  //     .unwrap();
  let mut display = Builder::ili9341_rgb565(di)
      .with_orientation(Orientation::Landscape(false))
      .init(&mut delay, Some(rst))
      .unwrap();

  info!("Turning on backlight...");
  backlight.set_high()?;

  info!("Clearing screen...");
  display.clear(Rgb565::new(0, 255, 0)).unwrap();

  info!("Drawing rectangle...");
  let fill = PrimitiveStyle::with_fill(Rgb565::new(255, 0, 0));
  Rectangle::new(Point::new(52, 10), Size::new(16, 16))
      .into_styled(fill)
      .draw(&mut display)
      .unwrap();

  info!("Sleeping 4s...");
  thread::sleep(Duration::from_secs(4));

  do_stuff(display).unwrap();
  Ok(())
}

pub struct DrawTargetProxy<D> {
  draw_target: D,
}

impl<C, D: DrawTarget<Color=C>> Dimensions for DrawTargetProxy<D> {
  fn bounding_box(&self) -> Rectangle {
    self.draw_target.bounding_box()
  }
}

impl <D: DrawTarget<Color=Rgb666>> DrawTarget for DrawTargetProxy<D> {
  type Color = Rgb565;
  type Error = D::Error;

  fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
  where I: IntoIterator<Item=Pixel<Self::Color>>
  {
    let converted = pixels.into_iter()
        .map(|p| {
          let input: Rgb565 = p.1;
          let output = Rgb666::new(input.r(), input.g(), input.b());
          Pixel(p.0, output)
        });
    self.draw_target.draw_iter(converted)
  }
}

// #[derive(PartialEq, Clone, Copy)]
// pub struct ColorProxy(Rgb565);
//
// impl From<Color for ColorProxy {
//   fn from(value: Color) -> Self {
//     let input = Rgb565::from(value);
//     Self(Rgb666::new(input.r(), input.g(), input.b()))
//   }
// }
//
// impl From<Rgb565> for ColorProxy {
//   fn from(value: Rgb565) -> Self {
//     let output = Rgb666::new(value.r(), value.g(), value.b());
//     ColorProxy(output)
//   }
// }
//
// impl PixelColor for ColorProxy {
//   type Raw = RawU16;
// }

// struct ColorProxy<INTERMEDIATE, TARGET> {
//   color: TARGET,
//   _phantom: PhantomData<INTERMEDIATE>,
// }
//
// impl <INTERMEDIATE, TARGET> From<Color> for ColorProxy<INTERMEDIATE, TARGET>
// where
//     INTERMEDIATE: From<Color> + RawData,
//     TARGET: RgbColor
// {
//   fn from(value: Color) -> Self {
//     let tmp = INTERMEDIATE::from(value);
//     tmp.into_inner() as u32;
//     let storage = tmp.into_storage() as u32;
//   }
// }

fn do_stuff(display: impl DrawTarget<Color = impl PixelColor + From<Color>>) -> LvResult<()> {
  let mut ui = UI::init()?;

  // Implement and register your display:
  ui.disp_drv_register(display)?;

  // Create screen and widgets
  let mut screen = ui.scr_act()?;

  let mut screen_style = Style::default();
  screen_style.set_bg_color(State::DEFAULT, Color::from_rgb((0, 0, 0)));
  screen.add_style(Part::Main, screen_style)?;

  // Create the gauge
  let mut gauge_style = Style::default();
  // Set a background color and a radius
  gauge_style.set_radius(State::DEFAULT, 5);
  gauge_style.set_bg_opa(State::DEFAULT, Opacity::OPA_COVER);
  gauge_style.set_bg_color(State::DEFAULT, Color::from_rgb((192, 192, 192)));
  // Set some paddings
  gauge_style.set_pad_inner(State::DEFAULT, 20);
  gauge_style.set_pad_top(State::DEFAULT, 20);
  gauge_style.set_pad_left(State::DEFAULT, 5);
  gauge_style.set_pad_right(State::DEFAULT, 5);

  gauge_style.set_scale_end_color(State::DEFAULT, Color::from_rgb((255, 0, 0)));
  gauge_style.set_line_color(State::DEFAULT, Color::from_rgb((255, 255, 255)));
  gauge_style.set_scale_grad_color(State::DEFAULT, Color::from_rgb((0, 0, 255)));
  gauge_style.set_line_width(State::DEFAULT, 2);
  gauge_style.set_scale_end_line_width(State::DEFAULT, 4);
  gauge_style.set_scale_end_border_width(State::DEFAULT, 4);

  let mut gauge = Gauge::new(&mut screen)?;
  gauge.add_style(Part::Main, gauge_style)?;
  gauge.set_align(&mut screen, Align::Center, 0, 0)?;
  gauge.set_value(0, 50)?;

  let mut i = 0;
  let mut loop_started = Instant::now();
  loop {
    gauge.set_value(0, i)?;

    ui.task_handler();

    ui.tick_inc(loop_started.elapsed());
    loop_started = Instant::now();

    Ets::delay_ms(1);
  }
}
