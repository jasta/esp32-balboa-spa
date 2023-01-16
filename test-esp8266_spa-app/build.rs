extern crate cc;

fn main() {
  cc::Build::new()
      .cpp(true)
      .define("EXCLUDE_MAIN", "1")
      .include("src/")
      .files([
        "src/esp8266_spa.cpp",
      ])
      .compile("libesp8266_spa.a");
}
