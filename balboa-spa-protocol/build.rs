extern crate cc;

fn main() {
  cc::Build::new()
      .cpp(true)
      .include("tests/")
      .files([
        "tests/esp8266_spa.cpp",
      ])
      .compile("esp8266_spa");
}
