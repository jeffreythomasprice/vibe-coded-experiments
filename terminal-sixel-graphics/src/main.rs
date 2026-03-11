use icy_sixel::SixelImage;

mod gradient {
    pub fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let hp = h / 60.0;
        let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
        let (r1, g1, b1) = match hp as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = l - c / 2.0;
        (
            ((r1 + m) * 255.0) as u8,
            ((g1 + m) * 255.0) as u8,
            ((b1 + m) * 255.0) as u8,
        )
    }

    pub fn generate_rainbow(count: usize) -> Vec<(u8, u8, u8)> {
        let count = count.min(256);
        (0..count)
            .map(|i| {
                let hue = (i as f64 / count as f64) * 360.0;
                hsl_to_rgb(hue, 1.0, 0.5)
            })
            .collect()
    }

    pub fn linear_gradient(
        rgba: &mut [u8],
        image_width: usize,
        rect: (usize, usize, usize, usize),
        from: (f64, f64),
        to: (f64, f64),
        colors: &[(u8, u8, u8)],
    ) {
        let dir = (to.0 - from.0, to.1 - from.1);
        let dir_len_sq = dir.0 * dir.0 + dir.1 * dir.1;
        let (rx, ry, rw, rh) = rect;

        for py in 0..rh {
            for px in 0..rw {
                let p = ((rx + px) as f64, (ry + py) as f64);
                let t = if dir_len_sq < 1e-12 {
                    0.0
                } else {
                    let dot = (p.0 - from.0) * dir.0 + (p.1 - from.1) * dir.1;
                    (dot / dir_len_sq).clamp(0.0, 1.0)
                };

                let seg = t * (colors.len() - 1) as f64;
                let idx = (seg as usize).min(colors.len() - 2);
                let frac = seg - idx as f64;
                let (r0, g0, b0) = colors[idx];
                let (r1, g1, b1) = colors[idx + 1];
                let r = r0 as f64 + (r1 as f64 - r0 as f64) * frac;
                let g = g0 as f64 + (g1 as f64 - g0 as f64) * frac;
                let b = b0 as f64 + (b1 as f64 - b0 as f64) * frac;

                let offset = ((ry + py) * image_width + (rx + px)) * 4;
                rgba[offset] = r as u8;
                rgba[offset + 1] = g as u8;
                rgba[offset + 2] = b as u8;
                rgba[offset + 3] = 255;
            }
        }
    }
}

use std::io::{Write, stdout};
use std::time::Duration;

use crossterm::{
    ExecutableCommand,
    cursor,
    event::{self, Event, KeyEventKind, MouseEventKind, EnableMouseCapture, DisableMouseCapture},
    terminal,
};

struct CleanupGuard;

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        let _ = stdout().execute(DisableMouseCapture);
        let _ = stdout().execute(cursor::Show);
        let _ = terminal::disable_raw_mode();
    }
}

fn draw_rect_outline(rgba: &mut [u8], img_w: usize, img_h: usize, cx: usize, cy: usize, size: usize) {
    let half = size / 2;
    let x0 = cx.saturating_sub(half);
    let y0 = cy.saturating_sub(half);
    let x1 = (cx + half).min(img_w.saturating_sub(1));
    let y1 = (cy + half).min(img_h.saturating_sub(1));

    let set_white = |rgba: &mut [u8], x: usize, y: usize| {
        let off = (y * img_w + x) * 4;
        rgba[off] = 255;
        rgba[off + 1] = 255;
        rgba[off + 2] = 255;
        rgba[off + 3] = 255;
    };

    for x in x0..=x1 {
        set_white(rgba, x, y0);
        set_white(rgba, x, y1);
    }
    for y in y0..=y1 {
        set_white(rgba, x0, y);
        set_white(rgba, x1, y);
    }
}

fn rect_bounds(cx: usize, cy: usize, img_w: usize, img_h: usize, size: usize) -> (usize, usize, usize, usize) {
    let half = size / 2;
    let x0 = cx.saturating_sub(half);
    let y0 = cy.saturating_sub(half);
    let x1 = (cx + half).min(img_w.saturating_sub(1));
    let y1 = (cy + half).min(img_h.saturating_sub(1));
    (x0, y0, x1, y1)
}

fn restore_region(pixels: &mut [u8], base: &[u8], img_w: usize, x0: usize, y0: usize, x1: usize, y1: usize) {
    for y in y0..=y1 {
        let row_start = (y * img_w + x0) * 4;
        let row_end = (y * img_w + x1) * 4 + 4;
        pixels[row_start..row_end].copy_from_slice(&base[row_start..row_end]);
    }
}

const RECT_SIZE: usize = 25;

fn render_frame(
    out: &mut impl Write,
    image: &mut SixelImage,
    base_rgba: &[u8],
    w: usize,
    h: usize,
    mouse_pos: Option<(usize, usize)>,
    last_rect: &mut Option<(usize, usize, usize, usize)>,
    skip_if_clean: bool,
) -> anyhow::Result<()> {
    if mouse_pos.is_none() && skip_if_clean {
        let sixel_data = image.encode()?;
        out.execute(cursor::MoveTo(0, 0))?;
        out.write_all(sixel_data.as_bytes())?;
        out.flush()?;
        return Ok(());
    }

    if let Some((lx0, ly0, lx1, ly1)) = *last_rect {
        restore_region(&mut image.pixels, base_rgba, w, lx0, ly0, lx1, ly1);
        *last_rect = None;
    } else if mouse_pos.is_some() {
        image.pixels.copy_from_slice(base_rgba);
    }

    if let Some((mx, my)) = mouse_pos {
        let bounds = rect_bounds(mx, my, w, h, RECT_SIZE);
        draw_rect_outline(&mut image.pixels, w, h, mx, my, RECT_SIZE);
        *last_rect = Some(bounds);
    }

    let sixel_data = image.encode()?;
    out.execute(cursor::MoveTo(0, 0))?;
    out.write_all(sixel_data.as_bytes())?;
    out.flush()?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    std::fs::create_dir_all("./logs")?;
    let file_appender =
        tracing_appender::rolling::never("./logs", "terminal-sixel-graphics.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_env_filter("warn,terminal_sixel_graphics=trace")
        .with_writer(non_blocking)
        .init();

    tracing::info!("starting terminal-sixel-graphics");

    let (cols, rows) = terminal::size()?;
    let (pixel_width, pixel_height) = match terminal::window_size() {
        Ok(size) if size.width > 0 && size.height > 0 => {
            tracing::debug!(
                px_w = size.width,
                px_h = size.height,
                "got pixel dimensions from terminal"
            );
            (size.width, size.height)
        }
        _ => {
            let fallback = (cols * 8, rows * 16);
            tracing::warn!(
                cols,
                rows,
                px_w = fallback.0,
                px_h = fallback.1,
                "pixel size unavailable, using fallback"
            );
            fallback
        }
    };

    let cell_width = pixel_width as f64 / cols as f64;
    let cell_height = pixel_height as f64 / rows as f64;

    tracing::info!(pixel_width, pixel_height, "rendering gradient");

    let w = pixel_width as usize;
    let h = pixel_height as usize;
    let colors = gradient::generate_rainbow(256);
    let mut base_rgba = vec![0u8; w * h * 4];
    gradient::linear_gradient(
        &mut base_rgba,
        w,
        (0, 0, w, h),
        (0.0, 0.0),
        (w as f64, h as f64),
        &colors,
    );

    let mut image = SixelImage::from_rgba(vec![0u8; w * h * 4], w, h);
    image.pixels.copy_from_slice(&base_rgba);

    terminal::enable_raw_mode()?;
    let _cleanup = CleanupGuard;

    let mut out = stdout();
    out.execute(cursor::Hide)?;
    out.execute(EnableMouseCapture)?;

    let mut last_rect: Option<(usize, usize, usize, usize)> = None;
    render_frame(&mut out, &mut image, &base_rgba, w, h, None, &mut last_rect, true)?;

    tracing::info!("gradient rendered, waiting for input");

    let mut last_pixel_pos: Option<(usize, usize)> = None;

    loop {
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                tracing::info!(?key, "key pressed, exiting");
                break;
            }
            Event::Mouse(mouse) => {
                let (col, row) = match mouse.kind {
                    MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                        (mouse.column, mouse.row)
                    }
                    _ => continue,
                };

                let mut px = (col as f64 * cell_width) as usize;
                let mut py = (row as f64 * cell_height) as usize;

                // Drain queued mouse events to avoid falling behind
                while event::poll(Duration::ZERO)? {
                    match event::read()? {
                        Event::Key(key) if key.kind == KeyEventKind::Press => {
                            tracing::info!(?key, "key pressed, exiting");
                            return Ok(());
                        }
                        Event::Mouse(m) => match m.kind {
                            MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                                px = (m.column as f64 * cell_width) as usize;
                                py = (m.row as f64 * cell_height) as usize;
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }

                px = px.min(w.saturating_sub(1));
                py = py.min(h.saturating_sub(1));
                let pos = (px, py);

                if last_pixel_pos == Some(pos) {
                    continue;
                }

                tracing::debug!(px, py, "mouse position");
                last_pixel_pos = Some(pos);
                render_frame(&mut out, &mut image, &base_rgba, w, h, Some(pos), &mut last_rect, false)?;
            }
            _ => {}
        }
    }

    Ok(())
}
