use cairo::{Context, ImageSurface};

use super::core::{
    blit_surface_rect, render_horizontal_slider, render_surface_sprite_spec, surface_from_xpm,
    RenderError, SliderRenderSpec,
};
use crate::skin::layout::{
    equalizer_control_spec, equalizer_slider_layout, EqualizerControl, EqualizerSlider,
    SliderLayout, EQUALIZER_WINDOW_HEIGHT, EQUALIZER_WINDOW_WIDTH, MAIN_TITLEBAR_HEIGHT,
};
use crate::skin::xpm::XpmImage;
use crate::skin::{DefaultSkin, SkinPixmapKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EqualizerRenderState {
    pub focused: bool,
    pub shaded: bool,
    pub active: bool,
    pub automatic: bool,
    pub pressed_control: Option<EqualizerControl>,
    pub preamp_position: i32,
    pub band_positions: [i32; 10],
    pub volume_position: i32,
    pub balance_position: i32,
}

impl Default for EqualizerRenderState {
    fn default() -> Self {
        Self {
            focused: true,
            shaded: false,
            active: true,
            automatic: false,
            pressed_control: None,
            preamp_position: 50,
            band_positions: [50; 10],
            volume_position: 94,
            balance_position: 19,
        }
    }
}

pub fn render_equalizer_background(
    cr: &Context,
    skin: &DefaultSkin,
    focused: bool,
    shaded: bool,
) -> Result<bool, RenderError> {
    if shaded {
        let Some(eq_ex) = skin.get(SkinPixmapKind::EqEx) else {
            return Ok(false);
        };
        let eq_ex = surface_from_xpm(eq_ex)?;
        return blit_surface_rect(
            cr,
            &eq_ex,
            0,
            if focused { 0 } else { 15 },
            0,
            0,
            EQUALIZER_WINDOW_WIDTH,
            MAIN_TITLEBAR_HEIGHT,
        );
    }

    let Some(eqmain_image) = skin.get(SkinPixmapKind::EqMain) else {
        return Ok(false);
    };
    let eqmain = surface_from_xpm(eqmain_image)?;
    let mut rendered = blit_surface_rect(
        cr,
        &eqmain,
        0,
        0,
        0,
        0,
        EQUALIZER_WINDOW_WIDTH,
        EQUALIZER_WINDOW_HEIGHT,
    )?;
    if eqmain_image.height() >= 163 {
        rendered |= blit_surface_rect(
            cr,
            &eqmain,
            0,
            if focused { 134 } else { 149 },
            0,
            0,
            EQUALIZER_WINDOW_WIDTH,
            MAIN_TITLEBAR_HEIGHT,
        )?;
    }
    Ok(rendered)
}

pub fn render_equalizer_state(
    cr: &Context,
    skin: &DefaultSkin,
    state: &EqualizerRenderState,
) -> Result<bool, RenderError> {
    let mut rendered = render_equalizer_background(cr, skin, state.focused, state.shaded)?;
    if state.shaded {
        rendered |= render_shaded_equalizer_sliders(cr, skin, state)?;
        return Ok(rendered);
    }

    let Some(eqmain_image) = skin.get(SkinPixmapKind::EqMain) else {
        return Ok(rendered);
    };
    let eqmain = surface_from_xpm(eqmain_image)?;

    rendered |= render_surface_sprite_spec(
        cr,
        &eqmain,
        equalizer_control_spec(
            EqualizerControl::On,
            state.active,
            state.pressed_control == Some(EqualizerControl::On),
        ),
    )?;
    rendered |= render_surface_sprite_spec(
        cr,
        &eqmain,
        equalizer_control_spec(
            EqualizerControl::Auto,
            state.automatic,
            state.pressed_control == Some(EqualizerControl::Auto),
        ),
    )?;
    rendered |= render_surface_sprite_spec(
        cr,
        &eqmain,
        equalizer_control_spec(
            EqualizerControl::Presets,
            false,
            state.pressed_control == Some(EqualizerControl::Presets),
        ),
    )?;

    rendered |= draw_eq_slider(
        cr,
        &eqmain,
        equalizer_slider_layout(EqualizerSlider::Preamp),
        state.preamp_position,
    )?;
    for (idx, position) in state.band_positions.iter().enumerate() {
        rendered |= draw_eq_slider(
            cr,
            &eqmain,
            equalizer_slider_layout(EqualizerSlider::Band(idx)),
            *position,
        )?;
    }

    draw_eq_graph(cr, eqmain_image, &state.band_positions)?;

    Ok(rendered)
}

fn render_shaded_equalizer_sliders(
    cr: &Context,
    skin: &DefaultSkin,
    state: &EqualizerRenderState,
) -> Result<bool, RenderError> {
    let volume_slider = equalizer_slider_layout(EqualizerSlider::ShadedVolume);
    let balance_slider = equalizer_slider_layout(EqualizerSlider::ShadedBalance);
    let volume_position = state
        .volume_position
        .clamp(volume_slider.min, volume_slider.max);
    let balance_position = state
        .balance_position
        .clamp(balance_slider.min, balance_slider.max);
    let mut rendered = render_horizontal_slider(
        cr,
        skin,
        SliderRenderSpec {
            kind: SkinPixmapKind::EqEx,
            x: volume_slider.rect.x,
            y: volume_slider.rect.y,
            width: volume_slider.rect.width,
            height: volume_slider.rect.height,
            position: volume_position,
            knob_source_x: shaded_eq_volume_knob_x(volume_position),
            knob_source_y: 30,
            knob_width: volume_slider.knob_size.width,
            knob_height: volume_slider.knob_size.height,
            frame_height: volume_slider.frame_height,
            frame_offset: volume_slider.frame_offset,
            frame: 1,
        },
    )?;
    rendered |= render_horizontal_slider(
        cr,
        skin,
        SliderRenderSpec {
            kind: SkinPixmapKind::EqEx,
            x: balance_slider.rect.x,
            y: balance_slider.rect.y,
            width: balance_slider.rect.width,
            height: balance_slider.rect.height,
            position: balance_position,
            knob_source_x: shaded_eq_balance_knob_x(balance_position),
            knob_source_y: 30,
            knob_width: balance_slider.knob_size.width,
            knob_height: balance_slider.knob_size.height,
            frame_height: balance_slider.frame_height,
            frame_offset: balance_slider.frame_offset,
            frame: 1,
        },
    )?;
    Ok(rendered)
}

fn shaded_eq_volume_knob_x(position: i32) -> i32 {
    match position {
        value if value < 32 => 1,
        value if value < 63 => 4,
        _ => 7,
    }
}

fn shaded_eq_balance_knob_x(position: i32) -> i32 {
    match position {
        value if value < 13 => 11,
        value if value < 26 => 14,
        _ => 17,
    }
}

fn draw_eq_slider(
    cr: &Context,
    eqmain: &ImageSurface,
    layout: SliderLayout,
    position: i32,
) -> Result<bool, RenderError> {
    let knob_y = layout.rect.y
        + (position.clamp(0, 100) * (layout.rect.height - layout.knob_size.height)) / 100;
    blit_surface_rect(
        cr,
        eqmain,
        0,
        164,
        layout.rect.x,
        knob_y,
        layout.knob_size.width,
        layout.knob_size.height,
    )
}

fn draw_eq_graph(
    cr: &Context,
    eqmain: &XpmImage,
    band_positions: &[i32; 10],
) -> Result<(), RenderError> {
    let points = eq_graph_points(band_positions);
    for pair in points.windows(2) {
        let (x0, y0) = pair[0];
        let (x1, y1) = pair[1];
        for y in y0.min(y1)..=y0.max(y1) {
            set_eq_graph_color(cr, eqmain, y);
            cr.rectangle(f64::from(x0), f64::from(17 + y), 1.0, 1.0);
            cr.fill()?;
        }
        if x1 != x0 {
            for x in x0.min(x1)..=x0.max(x1) {
                let t = f64::from(x - x0) / f64::from(x1 - x0);
                let y = (f64::from(y0) + (f64::from(y1 - y0) * t)).round() as i32;
                set_eq_graph_color(cr, eqmain, y);
                cr.rectangle(f64::from(x), f64::from(17 + y), 1.0, 1.0);
                cr.fill()?;
            }
        }
    }
    Ok(())
}

fn eq_graph_points(band_positions: &[i32; 10]) -> Vec<(i32, i32)> {
    band_positions
        .iter()
        .enumerate()
        .map(|(idx, position)| {
            let x = 88 + ((idx as i32 * 109) / 9);
            let value = 50 - (*position).clamp(0, 100);
            let y = (9 - ((value * 9) / 50)).clamp(0, 18);
            (x, y)
        })
        .collect()
}

fn set_eq_graph_color(cr: &Context, eqmain: &XpmImage, y: i32) {
    let color = eqmain
        .pixel_argb(115, (294 + y.clamp(0, 18)) as usize)
        .unwrap_or(0xff00_8cff);
    let red = ((color >> 16) & 0xff) as f64 / 255.0;
    let green = ((color >> 8) & 0xff) as f64 / 255.0;
    let blue = (color & 0xff) as f64 / 255.0;
    cr.set_source_rgb(red, green, blue);
}
