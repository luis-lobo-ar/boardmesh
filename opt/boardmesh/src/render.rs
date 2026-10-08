use crate::scene::{
    CanvasPoint, DrawableObject, ObjectKind, ResolvedConnectorPath, SceneColor, SceneDocument,
    SceneRect, TextObject,
};
use anyhow::{Result, anyhow, bail};
use font8x8::UnicodeFonts;
use image::{Rgba, RgbaImage};
use std::path::Path;

const DEFAULT_EMPTY_WIDTH: i32 = 1024;
const DEFAULT_EMPTY_HEIGHT: i32 = 768;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderScope {
    Scene,
    Selected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderResult {
    pub bounds: SceneRect,
    pub width: u32,
    pub height: u32,
    pub object_count: usize,
}

pub fn render_scene_png(
    scene: &SceneDocument,
    output_path: &Path,
    scope: RenderScope,
    padding: i32,
) -> Result<RenderResult> {
    let padding = padding.max(0);
    let (objects, content_bounds) = match scope {
        RenderScope::Scene => {
            let objects = scene.objects.iter().collect::<Vec<_>>();
            let bounds = scene.scene_bounds().unwrap_or(SceneRect::new(
                0,
                0,
                DEFAULT_EMPTY_WIDTH,
                DEFAULT_EMPTY_HEIGHT,
            ));
            (objects, bounds)
        }
        RenderScope::Selected => {
            let objects = scene.selected_objects();
            let Some(bounds) = scene.selected_bounds() else {
                bail!("no selected objects to render");
            };
            (objects, bounds)
        }
    };

    let crop = content_bounds.inflate(padding);
    let width = crop.width.max(1) as u32;
    let height = crop.height.max(1) as u32;
    let mut image = RgbaImage::from_pixel(width, height, rgba(scene.background));
    let origin = CanvasPoint::new(crop.x, crop.y);

    let object_count = objects.len();
    for object in &objects {
        draw_object(&mut image, scene, object, origin);
    }

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    image
        .save(output_path)
        .map_err(|error| anyhow!("failed to write png {}: {error}", output_path.display()))?;

    Ok(RenderResult {
        bounds: crop,
        width,
        height,
        object_count,
    })
}

pub fn render_scene_region_png(
    scene: &SceneDocument,
    output_path: &Path,
    object_ids: &[String],
    bounds: SceneRect,
    padding: i32,
) -> Result<RenderResult> {
    let objects = scene
        .objects
        .iter()
        .filter(|object| object_ids.iter().any(|object_id| object_id == &object.id))
        .collect::<Vec<_>>();

    render_objects_png(scene, output_path, &objects, bounds, padding)
}

fn render_objects_png(
    scene: &SceneDocument,
    output_path: &Path,
    objects: &[&DrawableObject],
    content_bounds: SceneRect,
    padding: i32,
) -> Result<RenderResult> {
    let padding = padding.max(0);
    let crop = content_bounds.inflate(padding);
    let width = crop.width.max(1) as u32;
    let height = crop.height.max(1) as u32;
    let mut image = RgbaImage::from_pixel(width, height, rgba(scene.background));
    let origin = CanvasPoint::new(crop.x, crop.y);

    for object in objects {
        draw_object(&mut image, scene, object, origin);
    }

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    image
        .save(output_path)
        .map_err(|error| anyhow!("failed to write png {}: {error}", output_path.display()))?;

    Ok(RenderResult {
        bounds: crop,
        width,
        height,
        object_count: objects.len(),
    })
}

fn draw_object(
    image: &mut RgbaImage,
    scene: &SceneDocument,
    object: &DrawableObject,
    origin: CanvasPoint,
) {
    match &object.kind {
        ObjectKind::Line { start, end } => draw_line(
            image,
            translate(*start, origin),
            translate(*end, origin),
            object.stroke.color,
            object.stroke.width,
        ),
        ObjectKind::Connector { .. } | ObjectKind::Link { .. } => {
            if let Some(path) = scene.resolved_connector_path(&object.id) {
                draw_connector_path(
                    image,
                    path,
                    origin,
                    object.stroke.color,
                    object.stroke.width,
                );
            }
        }
        ObjectKind::Rectangle { rect } => {
            let rect = SceneRect::new(
                rect.x - origin.x,
                rect.y - origin.y,
                rect.width,
                rect.height,
            );
            if let Some(fill) = object.fill {
                fill_rect(image, rect, fill);
            }
            stroke_rect(image, rect, object.stroke.color, object.stroke.width);
        }
        ObjectKind::Circle { center, radius } => {
            let center = translate(*center, origin);
            if let Some(fill) = object.fill {
                fill_circle(image, center, *radius, fill);
            }
            stroke_circle(
                image,
                center,
                *radius,
                object.stroke.color,
                object.stroke.width,
            );
        }
        ObjectKind::Triangle { triangle } => {
            let points = triangle
                .as_points()
                .iter()
                .map(|point| translate(*point, origin))
                .collect::<Vec<_>>();
            draw_polygon(
                image,
                &points,
                object.fill,
                object.stroke.color,
                object.stroke.width,
            );
        }
        ObjectKind::Polygon { points } => {
            let points = points
                .iter()
                .map(|point| translate(*point, origin))
                .collect::<Vec<_>>();
            draw_polygon(
                image,
                &points,
                object.fill,
                object.stroke.color,
                object.stroke.width,
            );
        }
        ObjectKind::Stroke { points } => {
            for segment in points.windows(2) {
                draw_line(
                    image,
                    translate(segment[0], origin),
                    translate(segment[1], origin),
                    object.stroke.color,
                    object.stroke.width,
                );
            }
        }
        ObjectKind::Text { text } => draw_text(image, text, origin, object.stroke.color),
    }
}

fn draw_connector_path(
    image: &mut RgbaImage,
    path: ResolvedConnectorPath,
    origin: CanvasPoint,
    color: SceneColor,
    stroke_width: i32,
) {
    match path {
        ResolvedConnectorPath::Polyline(points) => {
            for segment in points.windows(2) {
                draw_line(
                    image,
                    translate(segment[0], origin),
                    translate(segment[1], origin),
                    color,
                    stroke_width,
                );
            }
        }
        ResolvedConnectorPath::CubicBezier(segments) => {
            for segment in segments {
                draw_bezier_segment(image, segment, origin, color, stroke_width);
            }
        }
    }
}

fn draw_bezier_segment(
    image: &mut RgbaImage,
    segment: crate::scene::ResolvedCubicBezierSegment,
    origin: CanvasPoint,
    color: SceneColor,
    stroke_width: i32,
) {
    let points = approximate_bezier(segment);
    for pair in points.windows(2) {
        draw_line(
            image,
            translate(pair[0], origin),
            translate(pair[1], origin),
            color,
            stroke_width,
        );
    }
}

fn translate(point: CanvasPoint, origin: CanvasPoint) -> CanvasPoint {
    CanvasPoint::new(point.x - origin.x, point.y - origin.y)
}

fn draw_polygon(
    image: &mut RgbaImage,
    points: &[CanvasPoint],
    fill: Option<SceneColor>,
    stroke: SceneColor,
    stroke_width: i32,
) {
    if points.len() < 3 {
        return;
    }

    if let Some(fill) = fill {
        fill_polygon(image, points, fill);
    }

    for (start, end) in polygon_edges(points) {
        draw_line(image, start, end, stroke, stroke_width);
    }
}

fn draw_text(image: &mut RgbaImage, text: &TextObject, origin: CanvasPoint, color: SceneColor) {
    let start = translate(text.position, origin);
    let mut cursor_x = start.x;
    for ch in text.content.chars() {
        if ch == '\n' {
            cursor_x = start.x;
            continue;
        }
        if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits & (1 << col) != 0 {
                        blend_pixel(image, cursor_x + col, start.y + row as i32, rgba(color));
                    }
                }
            }
        }
        cursor_x += 8;
    }
}

fn fill_rect(image: &mut RgbaImage, rect: SceneRect, color: SceneColor) {
    let rgba = rgba(color);
    let x_start = rect.x.max(0);
    let y_start = rect.y.max(0);
    let x_end = (rect.x + rect.width).min(image.width() as i32);
    let y_end = (rect.y + rect.height).min(image.height() as i32);

    for y in y_start..y_end {
        for x in x_start..x_end {
            blend_pixel(image, x, y, rgba);
        }
    }
}

fn stroke_rect(image: &mut RgbaImage, rect: SceneRect, color: SceneColor, stroke_width: i32) {
    let top_left = CanvasPoint::new(rect.x, rect.y);
    let top_right = CanvasPoint::new(rect.x + rect.width, rect.y);
    let bottom_left = CanvasPoint::new(rect.x, rect.y + rect.height);
    let bottom_right = CanvasPoint::new(rect.x + rect.width, rect.y + rect.height);

    draw_line(image, top_left, top_right, color, stroke_width);
    draw_line(image, top_right, bottom_right, color, stroke_width);
    draw_line(image, bottom_right, bottom_left, color, stroke_width);
    draw_line(image, bottom_left, top_left, color, stroke_width);
}

fn fill_circle(image: &mut RgbaImage, center: CanvasPoint, radius: i32, color: SceneColor) {
    let rgba = rgba(color);
    let radius_squared = radius * radius;
    for y in (center.y - radius)..=(center.y + radius) {
        for x in (center.x - radius)..=(center.x + radius) {
            let dx = x - center.x;
            let dy = y - center.y;
            if dx * dx + dy * dy <= radius_squared {
                blend_pixel(image, x, y, rgba);
            }
        }
    }
}

fn stroke_circle(
    image: &mut RgbaImage,
    center: CanvasPoint,
    radius: i32,
    color: SceneColor,
    stroke_width: i32,
) {
    let rgba = rgba(color);
    let radius = radius.max(1);
    let inner = (radius - stroke_width.max(1)).max(0);
    let outer_sq = radius * radius;
    let inner_sq = inner * inner;
    for y in (center.y - radius)..=(center.y + radius) {
        for x in (center.x - radius)..=(center.x + radius) {
            let dx = x - center.x;
            let dy = y - center.y;
            let distance_sq = dx * dx + dy * dy;
            if distance_sq <= outer_sq && distance_sq >= inner_sq {
                blend_pixel(image, x, y, rgba);
            }
        }
    }
}

fn fill_polygon(image: &mut RgbaImage, points: &[CanvasPoint], color: SceneColor) {
    let Some(bounds) = points_bounds(points) else {
        return;
    };
    let rgba = rgba(color);
    for y in bounds.y..=(bounds.y + bounds.height) {
        for x in bounds.x..=(bounds.x + bounds.width) {
            if point_in_polygon(CanvasPoint::new(x, y), points) {
                blend_pixel(image, x, y, rgba);
            }
        }
    }
}

fn draw_line(
    image: &mut RgbaImage,
    start: CanvasPoint,
    end: CanvasPoint,
    color: SceneColor,
    stroke_width: i32,
) {
    let rgba = rgba(color);
    let mut x0 = start.x;
    let mut y0 = start.y;
    let x1 = end.x;
    let y1 = end.y;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let radius = (stroke_width.max(1) - 1) / 2;

    loop {
        draw_brush(image, x0, y0, radius, rgba);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = err * 2;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn draw_brush(image: &mut RgbaImage, x: i32, y: i32, radius: i32, color: Rgba<u8>) {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            blend_pixel(image, x + dx, y + dy, color);
        }
    }
}

fn blend_pixel(image: &mut RgbaImage, x: i32, y: i32, source: Rgba<u8>) {
    if x < 0 || y < 0 || x >= image.width() as i32 || y >= image.height() as i32 {
        return;
    }

    let destination = image.get_pixel_mut(x as u32, y as u32);
    let alpha = source[3] as f32 / 255.0;
    let inverse = 1.0 - alpha;
    let blended = [
        ((source[0] as f32 * alpha) + (destination[0] as f32 * inverse)).round() as u8,
        ((source[1] as f32 * alpha) + (destination[1] as f32 * inverse)).round() as u8,
        ((source[2] as f32 * alpha) + (destination[2] as f32 * inverse)).round() as u8,
        255,
    ];
    *destination = Rgba(blended);
}

fn rgba(color: SceneColor) -> Rgba<u8> {
    Rgba([color.r, color.g, color.b, color.a])
}

fn points_bounds(points: &[CanvasPoint]) -> Option<SceneRect> {
    let left = points.iter().map(|point| point.x).min()?;
    let top = points.iter().map(|point| point.y).min()?;
    let right = points.iter().map(|point| point.x).max()?;
    let bottom = points.iter().map(|point| point.y).max()?;
    Some(SceneRect::new(left, top, right - left, bottom - top))
}

fn polygon_edges(points: &[CanvasPoint]) -> Vec<(CanvasPoint, CanvasPoint)> {
    if points.len() < 2 {
        return Vec::new();
    }

    let mut edges = points
        .windows(2)
        .map(|segment| (segment[0], segment[1]))
        .collect::<Vec<_>>();
    edges.push((points[points.len() - 1], points[0]));
    edges
}

fn point_in_polygon(point: CanvasPoint, points: &[CanvasPoint]) -> bool {
    if points.len() < 3 {
        return false;
    }

    let mut inside = false;
    let mut previous = points[points.len() - 1];
    for current in points {
        let intersects = ((current.y > point.y) != (previous.y > point.y))
            && ((point.x as f64)
                < (previous.x - current.x) as f64 * (point.y - current.y) as f64
                    / (previous.y - current.y) as f64
                    + current.x as f64);
        if intersects {
            inside = !inside;
        }
        previous = *current;
    }
    inside
}

fn approximate_bezier(segment: crate::scene::ResolvedCubicBezierSegment) -> Vec<CanvasPoint> {
    const STEPS: usize = 24;
    (0..=STEPS)
        .map(|step| cubic_point(segment, step as f64 / STEPS as f64))
        .collect()
}

fn cubic_point(segment: crate::scene::ResolvedCubicBezierSegment, t: f64) -> CanvasPoint {
    let inv = 1.0 - t;
    let start_weight = inv * inv * inv;
    let control_start_weight = 3.0 * inv * inv * t;
    let control_end_weight = 3.0 * inv * t * t;
    let end_weight = t * t * t;

    CanvasPoint::new(
        (segment.start.x as f64 * start_weight
            + segment.control_start.x as f64 * control_start_weight
            + segment.control_end.x as f64 * control_end_weight
            + segment.end.x as f64 * end_weight)
            .round() as i32,
        (segment.start.y as f64 * start_weight
            + segment.control_start.y as f64 * control_start_weight
            + segment.control_end.y as f64 * control_end_weight
            + segment.end.y as f64 * end_weight)
            .round() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_scene_png_uses_scene_bounds_with_padding() {
        let mut scene = SceneDocument::default();
        scene.add_rectangle(
            SceneRect::new(10, 20, 40, 30),
            crate::scene::SceneStroke::default(),
            None,
        );

        let output =
            std::env::temp_dir().join(format!("boardmesh-render-{}.png", uuid::Uuid::new_v4()));
        let result = render_scene_png(&scene, &output, RenderScope::Scene, 8).unwrap();

        assert_eq!(result.bounds, SceneRect::new(2, 12, 56, 46));
        assert!(output.exists());
        let _ = std::fs::remove_file(output);
    }

    #[test]
    fn render_selected_png_requires_selection() {
        let scene = SceneDocument::default();
        let output =
            std::env::temp_dir().join(format!("boardmesh-render-{}.png", uuid::Uuid::new_v4()));

        assert!(render_scene_png(&scene, &output, RenderScope::Selected, 8).is_err());
    }

    #[test]
    fn render_scene_region_png_crops_to_explicit_bounds_and_object_ids() {
        let mut scene = SceneDocument::default();
        let inside = scene.add_rectangle(
            SceneRect::new(10, 20, 40, 30),
            crate::scene::SceneStroke::default(),
            None,
        );
        let _outside = scene.add_rectangle(
            SceneRect::new(300, 200, 50, 40),
            crate::scene::SceneStroke::default(),
            None,
        );

        let output = std::env::temp_dir().join(format!(
            "boardmesh-render-region-{}.png",
            uuid::Uuid::new_v4()
        ));
        let result = render_scene_region_png(
            &scene,
            &output,
            std::slice::from_ref(&inside),
            SceneRect::new(0, 0, 200, 120),
            10,
        )
        .unwrap();

        assert_eq!(result.object_count, 1);
        assert_eq!(result.bounds, SceneRect::new(-10, -10, 220, 140));
        assert!(output.exists());
        let _ = std::fs::remove_file(output);
    }
}
