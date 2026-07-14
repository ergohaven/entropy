//! Minimal streaming PDF writer for the layout image export.
//!
//! One A4 page per rendered layer image. Each page's image is produced on
//! demand, flate-compressed into a DeviceRGB image XObject and then dropped
//! before the next, so peak memory is a single page's RGBA buffer rather than
//! every page at once. Text is already rasterized into the images, so no font
//! embedding is needed and Cyrillic layer names render correctly.

use anyhow::Result;
use image::RgbaImage;
use std::io::Write as _;

const A4_LONG_PT: f32 = 841.89;
const A4_SHORT_PT: f32 = 595.276;
const PAGE_MARGIN_PT: f32 = 28.35;

/// Where a page image lands on its A4 sheet, in PostScript points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub page_w: f32,
    pub page_h: f32,
    pub draw_w: f32,
    pub draw_h: f32,
    pub draw_x: f32,
    pub draw_y: f32,
}

/// Orient the sheet to the image, then scale to fit inside the margins without
/// ever upscaling past 1 pt per pixel, and centre it.
fn place_on_a4(img_w: u32, img_h: u32) -> Placement {
    let (page_w, page_h) = if img_w >= img_h {
        (A4_LONG_PT, A4_SHORT_PT)
    } else {
        (A4_SHORT_PT, A4_LONG_PT)
    };
    let scale = ((page_w - PAGE_MARGIN_PT * 2.0) / img_w.max(1) as f32)
        .min((page_h - PAGE_MARGIN_PT * 2.0) / img_h.max(1) as f32)
        .min(1.0);
    let draw_w = img_w as f32 * scale;
    let draw_h = img_h as f32 * scale;
    Placement {
        page_w,
        page_h,
        draw_w,
        draw_h,
        draw_x: (page_w - draw_w) * 0.5,
        draw_y: (page_h - draw_h) * 0.5,
    }
}

/// Flate-compress an image's RGB bytes (export images are fully opaque, so the
/// alpha channel is dropped).
fn encode_rgb(image: &RgbaImage) -> Result<Vec<u8>> {
    let mut rgb = Vec::with_capacity(image.width() as usize * image.height() as usize * 3);
    for pixel in image.pixels() {
        rgb.extend_from_slice(&pixel.0[..3]);
    }
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&rgb)?;
    Ok(encoder.finish()?)
}

fn write_page(
    pdf: &mut Vec<u8>,
    offsets: &mut [usize],
    page: usize,
    image: &RgbaImage,
) -> Result<()> {
    let page_id = 3 + page * 3;
    let contents_id = page_id + 1;
    let image_id = page_id + 2;
    let p = place_on_a4(image.width(), image.height());

    offsets[page_id] = pdf.len();
    pdf.extend_from_slice(
        format!(
            "{page_id} 0 obj\n<< /Type /Page /Parent 2 0 R \
             /MediaBox [0 0 {:.2} {:.2}] \
             /Resources << /XObject << /Im{page} {image_id} 0 R >> >> \
             /Contents {contents_id} 0 R >>\nendobj\n",
            p.page_w, p.page_h
        )
        .as_bytes(),
    );

    let content = format!(
        "q\n{:.2} 0 0 {:.2} {:.2} {:.2} cm\n/Im{page} Do\nQ\n",
        p.draw_w, p.draw_h, p.draw_x, p.draw_y
    );
    offsets[contents_id] = pdf.len();
    pdf.extend_from_slice(
        format!(
            "{contents_id} 0 obj\n<< /Length {} >>\nstream\n{content}endstream\nendobj\n",
            content.len()
        )
        .as_bytes(),
    );

    let data = encode_rgb(image)?;
    offsets[image_id] = pdf.len();
    pdf.extend_from_slice(
        format!(
            "{image_id} 0 obj\n<< /Type /XObject /Subtype /Image \
             /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 \
             /Filter /FlateDecode /Length {} >>\nstream\n",
            image.width(),
            image.height(),
            data.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(&data);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    Ok(())
}

fn write_xref(pdf: &mut Vec<u8>, offsets: &[usize], object_count: usize) {
    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", object_count + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in &offsets[1..] {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
            object_count + 1
        )
        .as_bytes(),
    );
}

/// Build a PDF of `page_count` A4 pages, calling `render_page(i)` for each page
/// image just before it is encoded, so only one page's pixels are live at a time.
pub fn build_layer_pdf<F>(page_count: usize, mut render_page: F) -> Result<Vec<u8>>
where
    F: FnMut(usize) -> Result<RgbaImage>,
{
    anyhow::ensure!(page_count > 0, "no layers to export");

    // Objects: 1 = catalog, 2 = page tree, then (page, contents, image) per layer.
    let object_count = 2 + page_count * 3;
    let mut offsets = vec![0usize; object_count + 1];
    let mut pdf: Vec<u8> = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");

    offsets[1] = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    let kids = (0..page_count)
        .map(|page| format!("{} 0 R", 3 + page * 3))
        .collect::<Vec<_>>()
        .join(" ");
    offsets[2] = pdf.len();
    pdf.extend_from_slice(
        format!("2 0 obj\n<< /Type /Pages /Kids [{kids}] /Count {page_count} >>\nendobj\n")
            .as_bytes(),
    );

    for page in 0..page_count {
        let image = render_page(page)?;
        write_page(&mut pdf, &mut offsets, page, &image)?;
        drop(image);
    }

    write_xref(&mut pdf, &offsets, object_count);
    Ok(pdf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn img(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba([200, 60, 60, 255]))
    }

    #[test]
    fn builds_one_page_per_layer_with_orientation() {
        let sizes = [(200u32, 100u32), (100, 200)];
        let pdf = build_layer_pdf(sizes.len(), |i| Ok(img(sizes[i].0, sizes[i].1))).unwrap();
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Count 2"));
        assert!(text.contains("/MediaBox [0 0 841.89 595.28]")); // landscape for wide
        assert!(text.contains("/MediaBox [0 0 595.28 841.89]")); // portrait for tall
        assert_eq!(text.matches("/Subtype /Image").count(), 2);
    }

    #[test]
    fn rejects_empty_selection() {
        assert!(build_layer_pdf(0, |_| Ok(img(4, 4))).is_err());
    }

    #[test]
    fn renders_each_page_once_in_order() {
        // The builder pulls exactly one image per page, in order — the property
        // that lets it drop each before rendering the next (structurally, the
        // loop holds a single `image` binding, never a collection of them).
        let mut calls = Vec::new();
        let _pdf = build_layer_pdf(4, |page| {
            calls.push(page);
            Ok(img(10, 10))
        })
        .unwrap();
        assert_eq!(calls, vec![0, 1, 2, 3]);
    }

    #[test]
    fn render_error_aborts_the_build() {
        let result = build_layer_pdf(3, |page| {
            if page == 1 {
                anyhow::bail!("boom");
            }
            Ok(img(10, 10))
        });
        assert!(result.is_err());
    }

    #[test]
    fn placement_never_upscales_small_images() {
        // A tiny image must render at 1 pt/pixel, not stretched to the margins.
        let p = place_on_a4(20, 10);
        assert_eq!(p.draw_w, 20.0);
        assert_eq!(p.draw_h, 10.0);
        // Centered on a landscape sheet.
        assert!((p.draw_x - (p.page_w - 20.0) / 2.0).abs() < 0.001);
        assert!((p.draw_y - (p.page_h - 10.0) / 2.0).abs() < 0.001);
    }

    #[test]
    fn placement_fits_large_images_within_margins() {
        // A huge image is scaled down to fit inside the margins, aspect kept.
        let p = place_on_a4(4000, 2000);
        assert!(p.draw_w <= p.page_w - PAGE_MARGIN_PT * 2.0 + 0.01);
        assert!(p.draw_h <= p.page_h - PAGE_MARGIN_PT * 2.0 + 0.01);
        // One dimension is flush with its margin box (limiting axis).
        let fits_w = (p.draw_w - (p.page_w - PAGE_MARGIN_PT * 2.0)).abs() < 0.01;
        let fits_h = (p.draw_h - (p.page_h - PAGE_MARGIN_PT * 2.0)).abs() < 0.01;
        assert!(fits_w || fits_h);
        // Aspect ratio preserved.
        assert!((p.draw_w / p.draw_h - 2.0).abs() < 0.001);
        // Still centered.
        assert!((p.draw_x - (p.page_w - p.draw_w) / 2.0).abs() < 0.001);
        assert!((p.draw_y - (p.page_h - p.draw_h) / 2.0).abs() < 0.001);
    }

    #[test]
    fn xref_offsets_follow_trailer_and_cover_all_objects() {
        let pdf = build_layer_pdf(2, |_| Ok(img(4, 4))).unwrap();

        // Follow the trailer's startxref pointer to the xref table, as a reader
        // would — using raw bytes, since the header contains non-UTF-8 bytes.
        let needle = b"startxref\n";
        let pos = pdf
            .windows(needle.len())
            .rposition(|w| w == needle)
            .expect("startxref present")
            + needle.len();
        let line_end = pdf[pos..].iter().position(|&b| b == b'\n').unwrap();
        let xref_offset: usize = std::str::from_utf8(&pdf[pos..pos + line_end])
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(pdf[xref_offset..].starts_with(b"xref\n"));

        // From the xref keyword onward everything is ASCII.
        let xref = std::str::from_utf8(&pdf[xref_offset..]).unwrap();
        let object_count = 2 + 2 * 3; // catalog + pages + 3 per page

        // Header "xref\n0 N\n": N must equal object_count + 1.
        let declared: usize = xref
            .lines()
            .nth(1)
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(declared, object_count + 1);

        // One free entry + one entry per object, each pointing at "<id> 0 obj".
        let rows: Vec<&str> = xref.lines().skip(2).take(object_count + 1).collect();
        assert_eq!(rows.len(), object_count + 1);
        assert!(rows[0].starts_with("0000000000 65535 f"));
        for (id, row) in rows.iter().enumerate().skip(1) {
            let offset: usize = row[..10].parse().unwrap();
            let expected = format!("{id} 0 obj");
            assert!(
                pdf[offset..].starts_with(expected.as_bytes()),
                "xref entry {id} does not point at its object"
            );
        }
    }
}
