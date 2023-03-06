/// Provides a dedicated thread for debugging board state, possibly
/// logging to the serial console or an analytics service.
pub trait BoardMonitor {
  fn run_loop(self) -> anyhow::Result<()>;
}

pub struct NoopBoardMonitor;
impl BoardMonitor for NoopBoardMonitor {
  fn run_loop(self) -> anyhow::Result<()> {
    Ok(())
  }
}