extern crate cc;

fn main() {
  cc::Build::new()
      .cpp(true)
      .include("src/")
      .files([
        "src/esp8266_spa.cpp",
      ])
      .compile("esp8266_spa");
}
