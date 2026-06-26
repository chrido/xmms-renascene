use cairo::{Context, ImageSurface};

use super::core::{
    blit_surface_rect, render_horizontal_slider, render_surface_sprite_spec, surface_from_xpm,
    RenderError, SliderRenderSpec,
};
use crate::audio_model::{
    equalizer_position_to_db, EqualizerBandDb, EqualizerBandPositions, EQUALIZER_BANDS,
};
use crate::skin::layout::{
    equalizer_control_spec, equalizer_slider_layout, EqualizerControl, EqualizerSlider, SkinRect,
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
    pub pressed_slider: Option<EqualizerSlider>,
    pub preamp_position: i32,
    pub band_positions: EqualizerBandPositions,
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
            pressed_slider: None,
            preamp_position: 50,
            band_positions: [50; EQUALIZER_BANDS],
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
            SkinRect::new(
                0,
                if focused { 0 } else { 15 },
                EQUALIZER_WINDOW_WIDTH,
                MAIN_TITLEBAR_HEIGHT,
            ),
            (0, 0),
        );
    }

    let Some(eqmain_image) = skin.get(SkinPixmapKind::EqMain) else {
        return Ok(false);
    };
    let eqmain = surface_from_xpm(eqmain_image)?;
    let mut rendered = blit_surface_rect(
        cr,
        &eqmain,
        SkinRect::new(0, 0, EQUALIZER_WINDOW_WIDTH, EQUALIZER_WINDOW_HEIGHT),
        (0, 0),
    )?;
    if eqmain_image.height() >= 163 {
        rendered |= blit_surface_rect(
            cr,
            &eqmain,
            SkinRect::new(
                0,
                if focused { 134 } else { 149 },
                EQUALIZER_WINDOW_WIDTH,
                MAIN_TITLEBAR_HEIGHT,
            ),
            (0, 0),
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
        state.pressed_slider == Some(EqualizerSlider::Preamp),
    )?;
    for (idx, position) in state.band_positions.iter().enumerate() {
        let slider = EqualizerSlider::Band(idx);
        rendered |= draw_eq_slider(
            cr,
            &eqmain,
            equalizer_slider_layout(slider),
            *position,
            state.pressed_slider == Some(slider),
        )?;
    }

    draw_eq_graph(
        cr,
        &eqmain,
        eqmain_image,
        state.preamp_position,
        &state.band_positions,
    )?;

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
    pressed: bool,
) -> Result<bool, RenderError> {
    let slider_position = eq_slider_pixel_position(position);
    let frame = 27 - ((slider_position * 27) / 50);
    let (frame_x, frame_y) = if frame < 14 {
        ((frame * 15) + 13, 164)
    } else {
        (((frame - 14) * 15) + 13, 229)
    };
    let mut rendered = blit_surface_rect(
        cr,
        eqmain,
        SkinRect::new(frame_x, frame_y, layout.rect.width, layout.rect.height),
        (layout.rect.x, layout.rect.y),
    )?;
    rendered |= blit_surface_rect(
        cr,
        eqmain,
        SkinRect::new(
            0,
            if pressed { 176 } else { 164 },
            11,
            layout.knob_size.height,
        ),
        (
            layout.rect.x + ((layout.rect.width - layout.knob_size.width) / 2),
            layout.rect.y + slider_position,
        ),
    )?;
    Ok(rendered)
}

fn draw_eq_graph(
    cr: &Context,
    eqmain_surface: &ImageSurface,
    eqmain: &XpmImage,
    preamp_position: i32,
    band_positions: &EqualizerBandPositions,
) -> Result<(), RenderError> {
    blit_surface_rect(cr, eqmain_surface, SkinRect::new(0, 294, 113, 19), (86, 17))?;
    blit_surface_rect(
        cr,
        eqmain_surface,
        SkinRect::new(0, 314, 113, 1),
        (86, 17 + preamp_line_y(preamp_position)),
    )?;

    let x_values = [0.0, 11.0, 23.0, 35.0, 47.0, 59.0, 71.0, 83.0, 97.0, 109.0];
    let y_values = band_positions.map(equalizer_position_to_db);
    let y2 = spline_second_derivatives(&x_values, &y_values);
    let mut previous_y = 0;
    for x in 0..109 {
        let mut y =
            9 - ((eval_spline(&x_values, &y_values, &y2, f64::from(x)) * 9.0) / 20.0) as i32;
        y = y.clamp(0, 18);
        if x == 0 {
            previous_y = y;
        }
        for draw_y in y.min(previous_y)..=y.max(previous_y) {
            set_eq_graph_color(cr, eqmain, draw_y);
            cr.rectangle(f64::from(88 + x), f64::from(17 + draw_y), 1.0, 1.0);
            cr.fill()?;
        }
        previous_y = y;
    }
    Ok(())
}

fn eq_slider_pixel_position(position: i32) -> i32 {
    let pixel = position.clamp(0, 100) / 2;
    if (24..=26).contains(&pixel) {
        25
    } else {
        pixel
    }
}

fn preamp_line_y(position: i32) -> i32 {
    9 + ((equalizer_position_to_db(position) * 9.0) / 20.0) as i32
}

fn spline_second_derivatives(x: &EqualizerBandDb, y: &EqualizerBandDb) -> EqualizerBandDb {
    let mut y2 = [0.0; EQUALIZER_BANDS];
    let mut u = [0.0; EQUALIZER_BANDS];
    for i in 1..EQUALIZER_BANDS - 1 {
        let sig = (x[i] - x[i - 1]) / (x[i + 1] - x[i - 1]);
        let p = sig * y2[i - 1] + 2.0;
        y2[i] = (sig - 1.0) / p;
        u[i] = ((y[i + 1] - y[i]) / (x[i + 1] - x[i])) - ((y[i] - y[i - 1]) / (x[i] - x[i - 1]));
        u[i] = ((6.0 * u[i]) / (x[i + 1] - x[i - 1]) - sig * u[i - 1]) / p;
    }
    for k in (0..EQUALIZER_BANDS - 1).rev() {
        y2[k] = y2[k] * y2[k + 1] + u[k];
    }
    y2
}

fn eval_spline(xa: &EqualizerBandDb, ya: &EqualizerBandDb, y2a: &EqualizerBandDb, x: f64) -> f64 {
    let mut klo = 0;
    let mut khi = EQUALIZER_BANDS - 1;
    while khi - klo > 1 {
        let k = (khi + klo) >> 1;
        if xa[k] > x {
            khi = k;
        } else {
            klo = k;
        }
    }
    let h = xa[khi] - xa[klo];
    let a = (xa[khi] - x) / h;
    let b = (x - xa[klo]) / h;
    (a * ya[klo])
        + (b * ya[khi])
        + (((a * a * a - a) * y2a[klo] + (b * b * b - b) * y2a[khi]) * (h * h) / 6.0)
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
