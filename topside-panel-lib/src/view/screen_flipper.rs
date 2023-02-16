use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::hash::Hash;
use std::ptr;
use log::info;
use lvgl::{LvResult, Obj};

use crate::model::view_model::ViewModel;
use crate::view::loading_screen::LoadingScreen;
use crate::view::lvgl_ext::disp_load_scr;
use crate::view::main_screen::MainScreen;
use crate::view::provisioning_screen::ProvisioningScreen;

pub trait ScreenSelector {
  fn kind() -> &'static str;
  fn create() -> LvResult<BoxedScreen>;
  fn accept_model(model: &ViewModel) -> bool;
}

pub trait Screen {
  fn get_root(&self) -> &Obj;
  fn bind_model(&mut self, model: ViewModel) -> LvResult<()>;
}

pub type FactoryFn = dyn Fn() -> LvResult<BoxedScreen>;
pub type BoxedScreen = Box<dyn Screen>;

#[derive(Default)]
pub struct ScreenFlipper {
  active: Option<&'static str>,
  instances: HashMap<&'static str, BoxedScreen>,
}

impl ScreenFlipper {
  pub fn new() -> Self {
    Default::default()
  }

  pub fn bind_model(&mut self, model: ViewModel) -> LvResult<()> {
    let kind = self.select_screen(&model);
    let changed_screen = if self.active != Some(kind) {
      self.active = Some(kind);
      info!("Loading screen: {kind}");
      true
    } else { false };
    let screen = self.get_or_create_screen(kind)?;
    if changed_screen {
      disp_load_scr(screen.get_root())?;
    }
    screen.bind_model(model)
  }

  fn get_or_create_screen(&mut self, kind: &'static str) -> LvResult<&mut BoxedScreen> {
    if let Entry::Vacant(e) = self.instances.entry(kind) {
      e.insert(Self::create_screen(kind)?);
    }
    let instance = self.instances.get_mut(kind).unwrap();
    Ok(instance)
  }

  fn select_screen(&mut self, model: &ViewModel) -> &'static str {
    if ProvisioningScreen::accept_model(model) {
      ProvisioningScreen::kind()
    } else if MainScreen::accept_model(model) {
      MainScreen::kind()
    } else if LoadingScreen::accept_model(model) {
      LoadingScreen::kind()
    } else {
      panic!("No screen accepted the model!");
    }
  }

  fn create_screen(kind: &'static str) -> LvResult<BoxedScreen> {
    if ptr::eq(ProvisioningScreen::kind(), kind) {
      ProvisioningScreen::create()
    } else if ptr::eq(MainScreen::kind(), kind) {
      MainScreen::create()
    } else if ptr::eq(LoadingScreen::kind(), kind) {
      LoadingScreen::create()
    } else {
      panic!("No screen matches {kind}");
    }
  }
}

