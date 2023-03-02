use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::mpsc::{Sender, SyncSender};
use std::thread;
use std::time::{Duration, Instant};
use anyhow::anyhow;
use log::{error, info, warn};
use common_lib::view_model_event_handle::ViewEvent;
use crate::command::Command;
use crate::view_model::{ConnectionState, Mode, NominalModel, ProvisioningParams, TroubleAssociatingModel, UnprovisionedModel, ViewModel};
use crate::wifi_manager::{StaAssociationError, WifiDppBootstrapped, WifiManager};

/// Amount of time to allow for a successful connection before signaling to the UI that
/// something might be wrong.
const CONNECTING_GRACE_PERIOD: Duration = Duration::from_secs(60);

/// Time to wait between disconnect before attempting connect again.
const RECONNECT_DELAY: Duration = Duration::from_secs(1);

pub struct WifiHandler<W> {
  wifi_manager: W,
  model_manager: ModelManager,
}

struct ModelManager {
  view_events_tx: Sender<ViewEvent<ViewModel>>,
  state: AppState,
  last_model: Option<ViewModel>,
}

#[derive(Debug, Default)]
struct AppState {
  target_ssid: Option<String>,
  unrecoverable_error: Option<UnrecoverableError>,
  connection_state: ConnectionState,
  connection_stalled: Option<StaAssociationError>,
  waiting_for_dpp: Option<QrCode>,
}

#[derive(Debug, Clone)]
enum UnrecoverableError {
  WifiDriverFailed,
  DppBootstrap(String),
}

#[derive(Debug, Default)]
struct QrCode(String);

impl<'a, W: WifiManager<'a>> WifiHandler<W> {
  pub fn new(
      wifi_manager: W,
      view_events_tx: Sender<ViewEvent<ViewModel>>
  ) -> Self {
    Self {
      wifi_manager,
      model_manager: ModelManager {
        view_events_tx,
        state: Default::default(),
        last_model: None,
      }
    }
  }

  pub fn run_loop(mut self) -> anyhow::Result<()> {
    self.maybe_emit_view_model();
    if let Err((reported_e, actual_e)) = self.do_run_loop() {
      error!("Critical error {reported_e:?}: {actual_e}");
      self.state_mut().unrecoverable_error = Some(reported_e);
      self.maybe_emit_view_model();
      Err(anyhow!("{actual_e:?}"))
    } else {
      Ok(())
    }
  }

  fn do_run_loop(&mut self) -> Result<(), (UnrecoverableError, W::Error)> {
    info!("Loading Wi-Fi credentials...");
    let target = self.maybe_wait_for_config()?;
    self.maybe_emit_view_model();

    loop {
      info!("Connecting to {target}...");
      self.state_mut().connection_state = ConnectionState::Associating;
      self.maybe_emit_view_model();
      let initial_connection_time = Instant::now();
      while let Err(e) = self.wifi_manager.sta_connect() {
        warn!("Fail to connect to Wi-Fi: {e}");

        let time_since_first_try = initial_connection_time.elapsed();
        if time_since_first_try > CONNECTING_GRACE_PERIOD {
          warn!(
              "Time since last connection exceeded grace period: {}s!",
              time_since_first_try.as_secs());
          self.state_mut().connection_stalled = Some(e);
          self.maybe_emit_view_model();
        }
      }

      info!("Connected to {target}");
      self.state_mut().connection_stalled = None;
      self.state_mut().connection_state = ConnectionState::Connected;
      self.maybe_emit_view_model();
      self.wifi_manager.wait_while_connected().map_err(map_wifi_err::<W>)?;
      info!("Lost connection to {target}!");

      self.wait_for_reconnect();
    }
  }

  fn wait_for_reconnect(&mut self) {
    if !RECONNECT_DELAY.is_zero() {
      info!("Waiting for {}s to reconnect...", RECONNECT_DELAY.as_secs());
      self.state_mut().connection_state = ConnectionState::NotAssociated;
      self.maybe_emit_view_model();
      thread::sleep(RECONNECT_DELAY);
    }
  }

  fn maybe_wait_for_config(&mut self) -> Result<String, (UnrecoverableError, W::Error)> {
    self.wifi_manager.init().map_err(map_wifi_err::<W>)?;

    let network_name = match self.wifi_manager.get_sta_network_name().map_err(map_wifi_err::<W>)? {
      None => {
        info!("No credentials stored, preparing to use Wi-Fi Easy Connect...");
        let creds = {
          let dpp_bootstrapped =
              self.wifi_manager.dpp_bootstrap().map_err(map_dpp_err::<W>)?;

          info!("Generating QR code...");
          let qr_code = dpp_bootstrapped.get_qr_code().to_owned();

          let model_manager = &mut self.model_manager;
          model_manager.state.waiting_for_dpp = Some(QrCode(qr_code));
          model_manager.maybe_emit_view_model();

          info!("Got QR code, waiting for user to provision...");
          dpp_bootstrapped.listen_then_wait().map_err(map_dpp_err::<W>)?
        };

        self.wifi_manager.store_credentials(creds).map_err(map_wifi_err::<W>)?
      }
      Some(name) => name,
    };

    self.model_manager.state.waiting_for_dpp = None;
    self.state_mut().target_ssid = Some(network_name.clone());
    Ok(network_name)
  }

  fn state_mut(&mut self) -> &mut AppState {
    &mut self.model_manager.state
  }

  fn maybe_emit_view_model(&mut self) {
    self.model_manager.maybe_emit_view_model();
  }
}

impl ModelManager {
  pub fn maybe_emit_view_model(&mut self) {
    let model = self.state.generate_model();
    if self.last_model.as_ref() != Some(&model) {
      info!("Emitting new model: {model:?}");
      self.last_model = Some(model.clone());
      let _ = self.view_events_tx.send(ViewEvent::ModelUpdated(model));
    }
  }
}

impl AppState {
  fn generate_model(&self) -> ViewModel {
    // Order matters a lot here.  Must be informed by the logic in run_loop.
    let mode = if let Some(e) = &self.unrecoverable_error {
      Mode::UnrecoverableError(format!("{e:?}"))
    } else if let Some(target) = &self.target_ssid {
      if let Some(stalled_e) = &self.connection_stalled {
        Mode::TroubleAssociating(TroubleAssociatingModel {
          error: stalled_e.clone(),
        })
      } else {
        Mode::Nominal(NominalModel {
          network_name: target.clone(),
          connection_state: self.connection_state,
        })
      }
    } else if let Some(qr_code) = &self.waiting_for_dpp {
      Mode::NeedsProvisioning(UnprovisionedModel {
        params: ProvisioningParams {
          dpp_qr_code: qr_code.0.clone(),
        }
      })
    } else {
      Mode::Initializing
    };
    ViewModel { mode }
  }
}

fn map_wifi_err<'a, W: WifiManager<'a>>(e: W::Error) -> (UnrecoverableError, W::Error) {
  (UnrecoverableError::WifiDriverFailed, e)
}

fn map_dpp_err<'a, W: WifiManager<'a>>(e: W::Error) -> (UnrecoverableError, W::Error) {
  (UnrecoverableError::DppBootstrap(e.to_string()), e)
}