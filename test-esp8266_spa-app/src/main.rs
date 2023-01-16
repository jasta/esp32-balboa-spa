extern "C" {
  fn setup();
  fn r#loop();
}

fn main() {
  unsafe {
    setup();
    loop {
      r#loop();
    }
  }
}
