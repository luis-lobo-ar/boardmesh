use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct GraphPoint {
    pub x: f64,
    pub y: f64,
}

impl GraphPoint {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct GraphRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl GraphRect {
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn center(self) -> GraphPoint {
        GraphPoint::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AnchorKind {
    Center,
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Endpoint {
    Free(GraphPoint),
    Bound {
        node_id: String,
        anchor: AnchorKind,
        offset: GraphPoint,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConnectorKind {
    Straight,
    Orthogonal,
    Spline,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectorSpec {
    pub start: Endpoint,
    pub end: Endpoint,
    pub kind: ConnectorKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CubicBezierSegment {
    pub start: GraphPoint,
    pub control_start: GraphPoint,
    pub control_end: GraphPoint,
    pub end: GraphPoint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RoutedPath {
    Polyline(Vec<GraphPoint>),
    CubicBezier(Vec<CubicBezierSegment>),
}

pub fn resolve_anchor(rect: GraphRect, anchor: AnchorKind) -> GraphPoint {
    let center = rect.center();
    let left = rect.x;
    let right = rect.x + rect.width;
    let top = rect.y;
    let bottom = rect.y + rect.height;

    match anchor {
        AnchorKind::Center => center,
        AnchorKind::North => GraphPoint::new(center.x, top),
        AnchorKind::South => GraphPoint::new(center.x, bottom),
        AnchorKind::East => GraphPoint::new(right, center.y),
        AnchorKind::West => GraphPoint::new(left, center.y),
        AnchorKind::NorthEast => GraphPoint::new(right, top),
        AnchorKind::NorthWest => GraphPoint::new(left, top),
        AnchorKind::SouthEast => GraphPoint::new(right, bottom),
        AnchorKind::SouthWest => GraphPoint::new(left, bottom),
    }
}

pub fn resolve_endpoint<F>(endpoint: &Endpoint, lookup_bounds: F) -> Option<GraphPoint>
where
    F: Fn(&str) -> Option<GraphRect>,
{
    match endpoint {
        Endpoint::Free(point) => Some(*point),
        Endpoint::Bound {
            node_id,
            anchor,
            offset,
        } => {
            let anchor_point = resolve_anchor(lookup_bounds(node_id)?, *anchor);
            Some(GraphPoint::new(
                anchor_point.x + offset.x,
                anchor_point.y + offset.y,
            ))
        }
    }
}

pub fn route_connector<F>(connector: &ConnectorSpec, lookup_bounds: F) -> Option<RoutedPath>
where
    F: Fn(&str) -> Option<GraphRect>,
{
    let start = resolve_endpoint(&connector.start, &lookup_bounds)?;
    let end = resolve_endpoint(&connector.end, &lookup_bounds)?;

    let path = match connector.kind {
        ConnectorKind::Straight => RoutedPath::Polyline(vec![start, end]),
        ConnectorKind::Orthogonal => RoutedPath::Polyline(route_orthogonal(start, end)),
        ConnectorKind::Spline => RoutedPath::CubicBezier(vec![route_spline(start, end)]),
    };
    Some(path)
}

pub fn route_hits_point(path: &RoutedPath, point: GraphPoint, tolerance: f64) -> bool {
    match path {
        RoutedPath::Polyline(points) => points
            .windows(2)
            .any(|segment| distance_to_segment(point, segment[0], segment[1]) <= tolerance),
        RoutedPath::CubicBezier(segments) => segments.iter().any(|segment| {
            approximate_cubic_segments(*segment)
                .windows(2)
                .any(|segment_points| {
                    distance_to_segment(point, segment_points[0], segment_points[1]) <= tolerance
                })
        }),
    }
}

fn route_orthogonal(start: GraphPoint, end: GraphPoint) -> Vec<GraphPoint> {
    let mid_x = (start.x + end.x) / 2.0;
    vec![
        start,
        GraphPoint::new(mid_x, start.y),
        GraphPoint::new(mid_x, end.y),
        end,
    ]
}

fn route_spline(start: GraphPoint, end: GraphPoint) -> CubicBezierSegment {
    let mid_x = (start.x + end.x) / 2.0;
    CubicBezierSegment {
        start,
        control_start: GraphPoint::new(mid_x, start.y),
        control_end: GraphPoint::new(mid_x, end.y),
        end,
    }
}

fn distance_to_segment(point: GraphPoint, start: GraphPoint, end: GraphPoint) -> f64 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_squared = dx * dx + dy * dy;

    if length_squared == 0.0 {
        return distance(point, start);
    }

    let projection = ((point.x - start.x) * dx + (point.y - start.y) * dy) / length_squared;
    let clamped = projection.clamp(0.0, 1.0);
    let closest = GraphPoint::new(start.x + clamped * dx, start.y + clamped * dy);
    distance(point, closest)
}

fn distance(a: GraphPoint, b: GraphPoint) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}

fn approximate_cubic_segments(segment: CubicBezierSegment) -> Vec<GraphPoint> {
    const STEPS: usize = 24;
    (0..=STEPS)
        .map(|step| cubic_point(segment, step as f64 / STEPS as f64))
        .collect()
}

fn cubic_point(segment: CubicBezierSegment, t: f64) -> GraphPoint {
    let inv = 1.0 - t;
    let start_weight = inv * inv * inv;
    let control_start_weight = 3.0 * inv * inv * t;
    let control_end_weight = 3.0 * inv * t * t;
    let end_weight = t * t * t;

    GraphPoint::new(
        segment.start.x * start_weight
            + segment.control_start.x * control_start_weight
            + segment.control_end.x * control_end_weight
            + segment.end.x * end_weight,
        segment.start.y * start_weight
            + segment.control_start.y * control_start_weight
            + segment.control_end.y * control_end_weight
            + segment.end.y * end_weight,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn point(x: f64, y: f64) -> GraphPoint {
        GraphPoint::new(x, y)
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> GraphRect {
        GraphRect::new(x, y, width, height)
    }

    fn lookup<'a>(
        bounds: &'a BTreeMap<&'static str, GraphRect>,
    ) -> impl Fn(&str) -> Option<GraphRect> + 'a {
        move |node_id| bounds.get(node_id).copied()
    }

    #[test]
    fn resolves_anchor_positions_from_bounds() {
        let bounds = rect(10.0, 20.0, 80.0, 40.0);

        assert_eq!(
            resolve_anchor(bounds, AnchorKind::Center),
            point(50.0, 40.0)
        );
        assert_eq!(resolve_anchor(bounds, AnchorKind::North), point(50.0, 20.0));
        assert_eq!(resolve_anchor(bounds, AnchorKind::East), point(90.0, 40.0));
        assert_eq!(
            resolve_anchor(bounds, AnchorKind::SouthWest),
            point(10.0, 60.0)
        );
    }

    #[test]
    fn resolves_bound_endpoints_with_offsets() {
        let bounds = BTreeMap::from([("service", rect(100.0, 200.0, 60.0, 30.0))]);
        let endpoint = Endpoint::Bound {
            node_id: "service".to_string(),
            anchor: AnchorKind::East,
            offset: point(8.0, -4.0),
        };

        assert_eq!(
            resolve_endpoint(&endpoint, lookup(&bounds)),
            Some(point(168.0, 211.0))
        );
    }

    #[test]
    fn derives_straight_routes_as_two_point_polylines() {
        let connector = ConnectorSpec {
            start: Endpoint::Free(point(0.0, 0.0)),
            end: Endpoint::Free(point(120.0, 80.0)),
            kind: ConnectorKind::Straight,
        };

        assert_eq!(
            route_connector(&connector, |_| None),
            Some(RoutedPath::Polyline(vec![
                point(0.0, 0.0),
                point(120.0, 80.0)
            ]))
        );
    }

    #[test]
    fn derives_orthogonal_routes_using_a_single_bend() {
        let bounds = BTreeMap::from([
            ("left", rect(0.0, 0.0, 80.0, 60.0)),
            ("right", rect(220.0, 120.0, 80.0, 60.0)),
        ]);
        let connector = ConnectorSpec {
            start: Endpoint::Bound {
                node_id: "left".to_string(),
                anchor: AnchorKind::East,
                offset: point(0.0, 0.0),
            },
            end: Endpoint::Bound {
                node_id: "right".to_string(),
                anchor: AnchorKind::West,
                offset: point(0.0, 0.0),
            },
            kind: ConnectorKind::Orthogonal,
        };

        assert_eq!(
            route_connector(&connector, lookup(&bounds)),
            Some(RoutedPath::Polyline(vec![
                point(80.0, 30.0),
                point(150.0, 30.0),
                point(150.0, 150.0),
                point(220.0, 150.0),
            ]))
        );
    }

    #[test]
    fn derives_spline_routes_as_a_single_cubic_bezier_segment() {
        let connector = ConnectorSpec {
            start: Endpoint::Free(point(10.0, 10.0)),
            end: Endpoint::Free(point(110.0, 70.0)),
            kind: ConnectorKind::Spline,
        };

        assert_eq!(
            route_connector(&connector, |_| None),
            Some(RoutedPath::CubicBezier(vec![CubicBezierSegment {
                start: point(10.0, 10.0),
                control_start: point(60.0, 10.0),
                control_end: point(60.0, 70.0),
                end: point(110.0, 70.0),
            }]))
        );
    }

    #[test]
    fn detects_hits_near_polylines_and_rejects_distant_points() {
        let path = RoutedPath::Polyline(vec![point(0.0, 0.0), point(80.0, 0.0), point(80.0, 60.0)]);

        assert!(route_hits_point(&path, point(50.0, 3.0), 5.0));
        assert!(!route_hits_point(&path, point(50.0, 12.0), 5.0));
    }

    #[test]
    fn detects_hits_near_spline_segments() {
        let path = RoutedPath::CubicBezier(vec![CubicBezierSegment {
            start: point(10.0, 10.0),
            control_start: point(60.0, 10.0),
            control_end: point(60.0, 70.0),
            end: point(110.0, 70.0),
        }]);

        assert!(route_hits_point(&path, point(59.0, 40.0), 15.0));
        assert!(!route_hits_point(&path, point(20.0, 100.0), 10.0));
    }
}
