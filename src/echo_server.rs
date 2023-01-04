use embedded_svc::httpd::{Request, Response};
use embedded_svc::httpd::registry::Registry;
use esp_idf_svc::httpd::{Server, ServerRegistry};

pub fn start_rs485_echo_server() -> anyhow::Result<Server> {
  let server = ServerRegistry::new()
      .at("/send")
      .post(handle_send_request)?
      .at("/recv")
      .post(handle_recv_request)?;

  server.start(&Default::default())
}

fn handle_send_request(req: Request) -> anyhow::Result<Response> {
  Ok("foo".into())
}

fn handle_recv_request(req: Request) -> anyhow::Result<Response> {
  todo!()
}
