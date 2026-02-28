use tauri::image::Image;

const SIZE: u32 = 64;

fn render_svg(svg: &str) -> Image<'static> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())
        .expect("Failed to parse SVG");
    let size = tree.size();
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    Image::new_owned(pixmap.data().to_vec(), pixmap.width(), pixmap.height())
}

/// Green circle with white checkmark — no updates.
pub fn create_ok_icon() -> Image<'static> {
    render_svg(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
  <circle cx="32" cy="32" r="30" fill="#4CAF50"/>
  <polyline points="18,34 28,44 46,22" fill="none" stroke="white" stroke-width="6"
    stroke-linecap="round" stroke-linejoin="round"/>
</svg>"##,
    )
}

/// Orange circle with update count.
pub fn create_updates_icon(count: u32) -> Image<'static> {
    let text = if count <= 99 {
        count.to_string()
    } else {
        "99+".to_string()
    };
    let font_size = match text.len() {
        1 => 32,
        2 => 26,
        _ => 20,
    };
    render_svg(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
  <circle cx="32" cy="32" r="30" fill="#FF9800"/>
  <text x="32" y="32" text-anchor="middle" dominant-baseline="central"
    fill="white" font-family="sans-serif" font-size="{font_size}" font-weight="bold">{text}</text>
</svg>"##
    ))
}

/// Red circle with update count — restart needed.
pub fn create_restart_icon(count: u32) -> Image<'static> {
    let text = if count <= 99 {
        count.to_string()
    } else {
        "99+".to_string()
    };
    let font_size = match text.len() {
        1 => 32,
        2 => 26,
        _ => 20,
    };
    render_svg(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
  <circle cx="32" cy="32" r="30" fill="#F44336"/>
  <text x="32" y="32" text-anchor="middle" dominant-baseline="central"
    fill="white" font-family="sans-serif" font-size="{font_size}" font-weight="bold">{text}</text>
</svg>"##
    ))
}

/// Red circle with exclamation mark — reboot needed.
pub fn create_reboot_icon() -> Image<'static> {
    render_svg(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
  <circle cx="32" cy="32" r="30" fill="#F44336"/>
  <line x1="32" y1="14" x2="32" y2="36" stroke="white" stroke-width="6"
    stroke-linecap="round"/>
  <circle cx="32" cy="46" r="4" fill="white"/>
</svg>"##,
    )
}

/// Blue circle with circular arrow — checking in progress.
pub fn create_checking_icon() -> Image<'static> {
    render_svg(create_checking_svg(0.0).as_str())
}

/// Red circle with X — error.
pub fn create_error_icon() -> Image<'static> {
    render_svg(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
  <circle cx="32" cy="32" r="30" fill="#F44336"/>
  <line x1="22" y1="22" x2="42" y2="42" stroke="white" stroke-width="6"
    stroke-linecap="round"/>
  <line x1="42" y1="22" x2="22" y2="42" stroke="white" stroke-width="6"
    stroke-linecap="round"/>
</svg>"##,
    )
}

/// Generate checking SVG with the arrow rotated by the given angle (degrees).
fn create_checking_svg(angle: f64) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
  <circle cx="32" cy="32" r="30" fill="#2196F3"/>
  <g transform="rotate({angle} 32 32)">
    <path d="M 44.1 20.0 A 14 14 0 1 0 44.1 44.0"
      fill="none" stroke="white" stroke-width="4" stroke-linecap="round"/>
    <polygon points="44.1,44 50,38 50,48" fill="white"/>
  </g>
</svg>"##
    )
}

/// Generate rotated frames of the checking icon for spin animation.
pub fn create_checking_frames(count: usize) -> Vec<Image<'static>> {
    (0..count)
        .map(|i| {
            let angle = 360.0 * i as f64 / count as f64;
            render_svg(&create_checking_svg(angle))
        })
        .collect()
}

/// Create a scaled version of an icon for bounce animation.
pub fn create_scaled_icon(base: &Image<'_>, scale: f64) -> Image<'static> {
    let width = base.width();
    let height = base.height();
    let data = base.rgba();

    let new_w = (width as f64 * scale) as u32;
    let new_h = (height as f64 * scale) as u32;
    let offset_x = (width - new_w) / 2;
    let offset_y = (height - new_h) / 2;

    let mut out = vec![0u8; (SIZE * SIZE * 4) as usize];

    // Simple nearest-neighbor scaling
    for y in 0..new_h {
        for x in 0..new_w {
            let src_x = (x as f64 / scale) as u32;
            let src_y = (y as f64 / scale) as u32;
            if src_x < width && src_y < height {
                let src_idx = ((src_y * width + src_x) * 4) as usize;
                let dst_idx = (((y + offset_y) * SIZE + (x + offset_x)) * 4) as usize;
                if src_idx + 3 < data.len() && dst_idx + 3 < out.len() {
                    out[dst_idx..dst_idx + 4].copy_from_slice(&data[src_idx..src_idx + 4]);
                }
            }
        }
    }

    Image::new_owned(out, SIZE, SIZE)
}
