extern crate cc;

fn main() {
  cc::Build::new()
      .cpp(true)
      .files([
        "tests/CircularBuffer.tpp",
        "tests/esp8266_spa.cpp",
      ])
      .compile("libesp8266_spa.a");
}
