use std::thread;
use std::time::Duration;
use esp_idf_sys::MALLOC_CAP_DEFAULT;
use topside_panel_lib::app::status_printer::BoardMonitor;

pub struct EspStatusPrinter;

impl BoardMonitor for EspStatusPrinter {
  fn run_loop(self) -> anyhow::Result<()> {
    let interval = Duration::from_secs(2);
    loop {
      print_heap();
      thread::sleep(interval);
    }
  }
}

fn print_heap() {
  unsafe { esp_idf_sys::heap_caps_print_heap_info(MALLOC_CAP_DEFAULT) };
}
