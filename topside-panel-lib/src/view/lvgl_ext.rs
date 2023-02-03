use lvgl::style::Style;
use lvgl::{LvResult, NativeObject, State};
use crate::view::font::Font;

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
