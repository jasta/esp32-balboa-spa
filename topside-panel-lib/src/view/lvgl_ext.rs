use std::{mem, ptr};
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ptr::NonNull;
use lvgl::style::{Opacity, Style};
use lvgl::{Color, LvError, LvResult, NativeObject, State};
use lvgl::widgets::{Canvas, Label};
use lvgl_sys::{lv_color_int_t, lv_color_t, lv_coord_t, LV_IMG_CF_ALPHA_1BIT, LV_IMG_CF_ALPHA_2BIT, LV_IMG_CF_ALPHA_4BIT, LV_IMG_CF_ALPHA_8BIT, LV_IMG_CF_INDEXED_1BIT, LV_IMG_CF_INDEXED_2BIT, LV_IMG_CF_INDEXED_4BIT, LV_IMG_CF_INDEXED_8BIT, lv_img_cf_t, lv_img_dsc_t, LV_LABEL_LONG_BREAK, LV_LABEL_LONG_CROP, LV_LABEL_LONG_DOT, LV_LABEL_LONG_EXPAND, lv_label_long_mode_t, LV_LABEL_LONG_SROLL, LV_LABEL_LONG_SROLL_CIRC, LV_SCROLLBAR_MODE_AUTO};
use crate::view::font::Font;

pub fn disp_load_scr<C: NativeObject>(screen: &C) -> LvResult<()> {
  unsafe {
    lvgl_sys::lv_disp_load_scr(screen.raw()?.as_ptr());
  }
  Ok(())
}

pub fn label_set_long_mode(label: &mut Label, long_mode: LabelLongMode) -> LvResult<()> {
  unsafe {
    lvgl_sys::lv_label_set_long_mode(
        label.raw()?.as_ptr(),
        long_mode.raw());
  }
  Ok(())
}

pub enum LabelLongMode {
  Expand,
  Break,
  Dot,
  Scroll,
  ScrollCircular,
  Crop,
}

impl LabelLongMode {
  pub fn raw(&self) -> lv_label_long_mode_t {
    let raw = match self {
      LabelLongMode::Expand => LV_LABEL_LONG_EXPAND,
      LabelLongMode::Break => LV_LABEL_LONG_BREAK,
      LabelLongMode::Dot => LV_LABEL_LONG_DOT,
      LabelLongMode::Scroll => LV_LABEL_LONG_SROLL,
      LabelLongMode::ScrollCircular => LV_LABEL_LONG_SROLL_CIRC,
      LabelLongMode::Crop => LV_LABEL_LONG_CROP,
    };
    raw as lv_label_long_mode_t
  }
}

pub fn color_from_full(full: lv_color_int_t) -> Color {
  let x = lv_color_t { full };
  Color::from_raw(x)
}

#[derive(Clone)]
pub struct Image<'d> {
  raw: *mut lvgl_sys::lv_img_dsc_t,
  _phantom: PhantomData<&'d ()>,
}

impl<'d> Image<'d> {
  pub fn get_width(&self) -> lv_coord_t {
    let width = unsafe { (*self.raw).header.w() };
    lv_coord_t::try_from(width).unwrap()
  }

  pub fn get_height(&self) -> lv_coord_t {
    let height = unsafe { (*self.raw).header.h() };
    lv_coord_t::try_from(height).unwrap()
  }
}

pub fn canvas_get_img(canvas: &Canvas) -> LvResult<Image<'_>> {
  let img = unsafe {
    let raw = lvgl_sys::lv_canvas_get_img(canvas.raw()?.as_ptr());
    match ptr::NonNull::new(raw) {
      None => Err(LvError::InvalidReference),
      Some(p) => Ok(p),
    }
  };
  img.map(|p| {
    Image {
      raw: p.as_ptr(),
      _phantom: PhantomData,
    }
  })
}

pub fn canvas_set_size(canvas: &mut Canvas, width: lv_coord_t, height: lv_coord_t, format: ImgColorFormat) -> LvResult<()> {
  let new_size = format.compute_buf_size(width, height)
      .ok_or(LvError::LvOOMemory)?;
  unsafe {
    let new_buf = lvgl_sys::lv_mem_alloc(new_size);
    lvgl_sys::lv_canvas_set_buffer(
        canvas.raw()?.as_ptr(),
        new_buf,
        width,
        height,
        format.raw());
  }
  Ok(())
}

pub enum ImgColorFormat {
  Alpha1Bit,
  Alpha2Bit,
  Alpha4Bit,
  Alpha8Bit,
  Indexed1Bit,
  Indexed2Bit,
  Indexed4Bit,
  Indexed8Bit,
}

impl ImgColorFormat {
  pub fn raw(&self) -> lv_img_cf_t {
    let raw = match self {
      ImgColorFormat::Alpha1Bit => LV_IMG_CF_ALPHA_1BIT,
      ImgColorFormat::Alpha2Bit => LV_IMG_CF_ALPHA_2BIT,
      ImgColorFormat::Alpha4Bit => LV_IMG_CF_ALPHA_4BIT,
      ImgColorFormat::Alpha8Bit => LV_IMG_CF_ALPHA_8BIT,
      ImgColorFormat::Indexed1Bit => LV_IMG_CF_INDEXED_1BIT,
      ImgColorFormat::Indexed2Bit => LV_IMG_CF_INDEXED_2BIT,
      ImgColorFormat::Indexed4Bit => LV_IMG_CF_INDEXED_4BIT,
      ImgColorFormat::Indexed8Bit => LV_IMG_CF_INDEXED_8BIT,
    };
    raw as lv_img_cf_t
  }

  pub fn compute_buf_size(&self, width: lv_coord_t, height: lv_coord_t) -> Option<usize> {
    let width_sane = usize::try_from(width).ok()?;
    let height_sane = usize::try_from(height).ok()?;

    // This comes from LV_IMG_BUF_SIZE_* macros
    let (divisor, index_size) = match self {
      ImgColorFormat::Alpha1Bit => (8, 0),
      ImgColorFormat::Alpha2Bit => (4, 0),
      ImgColorFormat::Alpha4Bit => (2, 0),
      ImgColorFormat::Alpha8Bit => (1, 0),
      ImgColorFormat::Indexed1Bit => (8, 4*2),
      ImgColorFormat::Indexed2Bit => (4, 4*4),
      ImgColorFormat::Indexed4Bit => (2, 4*16),
      ImgColorFormat::Indexed8Bit => (1, 4*256),
    };
    let alpha_size = ((width_sane / divisor) + 1).checked_mul(height_sane);
    alpha_size.and_then(|x| {
      x.checked_add(index_size)
    })
  }
}

pub fn canvas_fill_bg(canvas: &mut Canvas, color: Color, opacity: Opacity) -> LvResult<()> {
  unsafe {
    lvgl_sys::lv_canvas_fill_bg(
        canvas.raw()?.as_ptr(),
        color.raw(),
        opacity.bits() as lvgl_sys::lv_opa_t);
  }
  Ok(())
}

pub fn canvas_set_palette(canvas: &mut Canvas, id: u8, color: Color) -> LvResult<()> {
  unsafe {
    lvgl_sys::lv_canvas_set_palette(
        canvas.raw()?.as_ptr(),
        id,
        color.raw());
  }
  Ok(())
}

pub fn canvas_set_px(canvas: &mut Canvas, x: lv_coord_t, y: lv_coord_t, color: &Color) -> LvResult<()> {
  unsafe {
    lvgl_sys::lv_canvas_set_px(
        canvas.raw()?.as_ptr(),
        x,
        y,
        color.raw());
  }
  Ok(())
}

pub fn style_set_text_font(style: &mut Style, state: State, value: Font) {
  let native_state: u32 = state.get_bits();
  unsafe {
    lvgl_sys::_lv_style_set_ptr(
      style.raw(),
      (lvgl_sys::LV_STYLE_TEXT_FONT as u32
          | (native_state << lvgl_sys::LV_STYLE_STATE_POS as u32)) as u16,
      value.raw() as *mut cty::c_void,
    );
  }
}

pub fn obj_set_auto_realign<C>(obj: &mut C, value: bool) -> LvResult<()>
where
    C: NativeObject,
{
  unsafe {
    lvgl_sys::lv_obj_set_auto_realign(
      obj.raw()?.as_mut(),
      value);
  }
  Ok(())
}

pub fn obj_get_width(obj: &impl NativeObject) -> LvResult<lv_coord_t> {
  let retval = unsafe {
    lvgl_sys::lv_obj_get_width(obj.raw()?.as_ptr())
  };
  Ok(retval)
}

pub fn obj_get_height(obj: &impl NativeObject) -> LvResult<lv_coord_t> {
  let retval = unsafe {
    lvgl_sys::lv_obj_get_height(obj.raw()?.as_ptr())
  };
  Ok(retval)
}

pub struct Anim {
  raw: Box<lvgl_sys::lv_anim_t>,
}

impl Anim {
  pub fn new() -> LvResult<Anim> {
    let raw = unsafe {
      let mut anim = mem::MaybeUninit::<lvgl_sys::lv_anim_t>::uninit();
      lvgl_sys::lv_anim_init(anim.as_mut_ptr());
      Box::new(anim.assume_init())
    };
    Ok(Anim { raw })
  }
}