//! Synchronous Activity layout query and compact-protocol validation.
//!
//! The `long[13]` payload is deliberately retained: it is fixed-size, allocation
//! light, decoded through named indexes, and every value is range-checked before
//! entering the Rust layout state machine.

use jni::objects::JLongArray;

use super::super::android_runtime::AndroidSystemInsets;
use super::activity;

#[derive(Debug, Clone, Copy)]
pub struct AndroidWindowLayoutSnapshot {
    pub width: i32,
    pub height: i32,
    pub orientation: i32,
    pub insets: AndroidSystemInsets,
    pub inset_width: i32,
    pub inset_height: i32,
    pub inset_orientation: i32,
    pub config_generation: i64,
    pub inset_generation: i64,
    pub insets_fresh: bool,
}

const LAYOUT_WIDTH: usize = 0;
const LAYOUT_HEIGHT: usize = 1;
const LAYOUT_ORIENTATION: usize = 2;
const LAYOUT_INSET_LEFT: usize = 3;
const LAYOUT_INSET_TOP: usize = 4;
const LAYOUT_INSET_RIGHT: usize = 5;
const LAYOUT_INSET_BOTTOM: usize = 6;
const LAYOUT_INSET_WIDTH: usize = 7;
const LAYOUT_INSET_HEIGHT: usize = 8;
const LAYOUT_INSET_ORIENTATION: usize = 9;
const LAYOUT_CONFIG_GENERATION: usize = 10;
const LAYOUT_INSET_GENERATION: usize = 11;
const LAYOUT_INSETS_FRESH: usize = 12;
const LAYOUT_FIELD_COUNT: usize = 13;

pub fn window_layout_snapshot_pixels() -> Option<AndroidWindowLayoutSnapshot> {
    let context = activity::context().ok()?;
    let context = context.as_ref()?;
    let mut env = context.vm.attach_current_thread().ok()?;
    let array = env
        .call_method(
            context.activity.as_obj(),
            "windowLayoutSnapshot",
            "()[J",
            &[],
        )
        .and_then(|value| value.l())
        .ok()
        .map(JLongArray::from)?;
    if env.get_array_length(&array).ok()? != LAYOUT_FIELD_COUNT as i32 {
        return None;
    }
    let mut values = [0_i64; LAYOUT_FIELD_COUNT];
    env.get_long_array_region(&array, 0, &mut values).ok()?;
    Some(AndroidWindowLayoutSnapshot {
        width: i32::try_from(values[LAYOUT_WIDTH]).ok()?,
        height: i32::try_from(values[LAYOUT_HEIGHT]).ok()?,
        orientation: i32::try_from(values[LAYOUT_ORIENTATION]).ok()?,
        insets: AndroidSystemInsets {
            left: i32::try_from(values[LAYOUT_INSET_LEFT]).ok()?,
            top: i32::try_from(values[LAYOUT_INSET_TOP]).ok()?,
            right: i32::try_from(values[LAYOUT_INSET_RIGHT]).ok()?,
            bottom: i32::try_from(values[LAYOUT_INSET_BOTTOM]).ok()?,
        },
        inset_width: i32::try_from(values[LAYOUT_INSET_WIDTH]).ok()?,
        inset_height: i32::try_from(values[LAYOUT_INSET_HEIGHT]).ok()?,
        inset_orientation: i32::try_from(values[LAYOUT_INSET_ORIENTATION]).ok()?,
        config_generation: values[LAYOUT_CONFIG_GENERATION],
        inset_generation: values[LAYOUT_INSET_GENERATION],
        insets_fresh: values[LAYOUT_INSETS_FRESH] != 0,
    })
}
