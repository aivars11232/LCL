//! A pairing link as a QR code: for a terminal, and as an SVG image.

use qrcode::{Color, EcLevel, QrCode};

/// The code's modules, dark as `true`, row by row.
pub fn matrix(text: &str) -> Result<(usize, Vec<bool>), String> {
    let code = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::M)
        .map_err(|e| format!("the link does not fit a QR code: {e}"))?;
    let width = code.width();
    let dark = code
        .to_colors()
        .into_iter()
        .map(|c| c == Color::Dark)
        .collect();
    Ok((width, dark))
}

/// Two rows per text line, with a quiet zone, for a terminal: dark modules
/// are drawn as spaces on a light background so any terminal theme scans.
pub fn terminal(text: &str) -> Result<String, String> {
    let (width, dark) = matrix(text)?;
    let quiet = 2;
    let at = |x: isize, y: isize| {
        x >= 0
            && y >= 0
            && (x as usize) < width
            && (y as usize) < width
            && dark[y as usize * width + x as usize]
    };
    let mut out = String::new();
    let size = width as isize;
    let mut y = -(quiet as isize);
    while y < size + quiet as isize {
        for x in -(quiet as isize)..size + quiet as isize {
            // Upper half and lower half of this character cell.
            out.push(match (at(x, y), at(x, y + 1)) {
                (false, false) => '█',
                (false, true) => '▀',
                (true, false) => '▄',
                (true, true) => ' ',
            });
        }
        out.push('\n');
        y += 2;
    }
    Ok(out)
}

/// A standalone SVG, black on white with a four-module quiet zone.
pub fn svg(text: &str) -> Result<String, String> {
    let (width, dark) = matrix(text)?;
    let size = width + 8;
    let mut path = String::new();
    for y in 0..width {
        for x in 0..width {
            if dark[y * width + x] {
                path.push_str(&format!("M{} {}h1v1h-1z", x + 4, y + 4));
            }
        }
    }
    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {size} {size}\" shape-rendering=\"crispEdges\">\
         <rect width=\"{size}\" height=\"{size}\" fill=\"#fff\"/><path d=\"{path}\" fill=\"#000\"/></svg>"
    ))
}
