use esp_idf_svc::httpd::{Server, ServerRegistry};
use embedded_svc::httpd::registry::Registry;

pub fn start_rs485_echo_server() -> anyhow::Result<Server> {
  let server = ServerRegistry::new()
      .at("/send")
      .post(|req| {
        Ok("foo".into())
      })?;

  server.start(&Default::default())
}
