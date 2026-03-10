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

use crossterm::{
    ExecutableCommand,
    cursor,
    event::{self, Event, KeyEventKind},
    terminal,
};

struct CleanupGuard;

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        let _ = stdout().execute(cursor::Show);
        let _ = terminal::disable_raw_mode();
    }
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
            let (cols, rows) = terminal::size()?;
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

    tracing::info!(pixel_width, pixel_height, "rendering gradient");

    let w = pixel_width as usize;
    let h = pixel_height as usize;
    let colors = gradient::generate_rainbow(256);
    let mut rgba = vec![0u8; w * h * 4];
    gradient::linear_gradient(
        &mut rgba,
        w,
        (0, 0, w, h),
        (0.0, 0.0),
        (w as f64, h as f64),
        &colors,
    );
    let image = SixelImage::from_rgba(rgba, w, h);
    let sixel_data = image.encode()?;

    terminal::enable_raw_mode()?;
    let _cleanup = CleanupGuard;

    let mut out = stdout();
    out.execute(cursor::Hide)?;
    out.execute(cursor::MoveTo(0, 0))?;
    write!(out, "{}", sixel_data)?;
    out.flush()?;

    tracing::info!("gradient rendered, waiting for keypress");

    loop {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                tracing::info!(?key, "key pressed, exiting");
                break;
            }
        }
    }

    Ok(())
}
