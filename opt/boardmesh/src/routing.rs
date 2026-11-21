use mesh_graph::{
    AnchorKind, ConnectorKind, ConnectorSpec, Endpoint, GraphPoint, GraphRect, RoutedPath,
};
use ortho_router::{Connector, Options, Point, Port, Rect, Router, Side};

// Larger scenes retain the existing route instead of building an unbounded grid per paint.
const MAX_OBSTACLES: usize = 32;

pub(crate) fn avoid_obstacles(
    connector: &ConnectorSpec,
    legacy: RoutedPath,
    obstacles: impl Iterator<Item = GraphRect>,
) -> RoutedPath {
    if connector.kind != ConnectorKind::Orthogonal {
        return legacy;
    }
    let RoutedPath::Polyline(points) = &legacy else {
        return legacy;
    };
    let (Some(&start), Some(&end)) = (points.first(), points.last()) else {
        return legacy;
    };
    let obstacles: Vec<_> = obstacles
        .filter(|rect| valid_rect(*rect) && !contains(*rect, start) && !contains(*rect, end))
        .take(MAX_OBSTACLES + 1)
        .collect();
    if obstacles.len() > MAX_OBSTACLES || !obstacles.iter().any(|rect| crosses(points, *rect)) {
        return legacy;
    }
    let Some(local_end) = local_point(end, start) else {
        return legacy;
    };
    let Some(local_obstacles): Option<Vec<_>> = obstacles
        .iter()
        .map(|rect| {
            Some(Rect::from_corners(
                local_point(GraphPoint::new(rect.x, rect.y), start)?,
                local_point(
                    GraphPoint::new(rect.x + rect.width, rect.y + rect.height),
                    start,
                )?,
            ))
        })
        .collect()
    else {
        return legacy;
    };
    let mut router = Router::new(Options::default()).with_obstacles(local_obstacles);
    let route = router.route(&Connector::new(
        Port::new([0.0, 0.0], side(&connector.start, start, end)),
        Port::new(local_end, side(&connector.end, end, start)),
    ));
    if !route.routed {
        return legacy;
    }
    let current: Vec<_> = route
        .points
        .iter()
        .map(|point| GraphPoint::new(f64::from(point.x) + start.x, f64::from(point.y) + start.y))
        .collect();
    if current.len() < 2
        || current.first() != Some(&start)
        || current.last() != Some(&end)
        || current
            .iter()
            .any(|point| !point.x.is_finite() || !point.y.is_finite())
        || current
            .windows(2)
            .any(|pair| pair[0].x != pair[1].x && pair[0].y != pair[1].y)
        || obstacles.iter().any(|rect| crosses(&current, *rect))
    {
        return legacy;
    }
    RoutedPath::Polyline(current)
}

fn local_point(point: GraphPoint, origin: GraphPoint) -> Option<Point> {
    let x = point.x - origin.x;
    let y = point.y - origin.y;
    let local = Point::new(x as f32, y as f32);
    (local.is_finite() && f64::from(local.x) == x && f64::from(local.y) == y).then_some(local)
}

fn valid_rect(rect: GraphRect) -> bool {
    [rect.x, rect.y, rect.width, rect.height]
        .iter()
        .all(|value| value.is_finite())
        && rect.width > 0.0
        && rect.height > 0.0
}

fn contains(rect: GraphRect, point: GraphPoint) -> bool {
    point.x > rect.x
        && point.x < rect.x + rect.width
        && point.y > rect.y
        && point.y < rect.y + rect.height
}

fn crosses(points: &[GraphPoint], rect: GraphRect) -> bool {
    points.windows(2).any(|pair| {
        let (a, b) = (pair[0], pair[1]);
        if a.y == b.y {
            a.y > rect.y
                && a.y < rect.y + rect.height
                && a.x.min(b.x) < rect.x + rect.width
                && a.x.max(b.x) > rect.x
        } else if a.x == b.x {
            a.x > rect.x
                && a.x < rect.x + rect.width
                && a.y.min(b.y) < rect.y + rect.height
                && a.y.max(b.y) > rect.y
        } else {
            true
        }
    })
}

fn side(endpoint: &Endpoint, from: GraphPoint, to: GraphPoint) -> Side {
    let horizontal = (to.x - from.x).abs() >= (to.y - from.y).abs();
    let anchor = match endpoint {
        Endpoint::Bound { anchor, .. } => Some(*anchor),
        _ => None,
    };
    match anchor {
        Some(AnchorKind::North) => Side::Top,
        Some(AnchorKind::South) => Side::Bottom,
        Some(AnchorKind::East) => Side::Right,
        Some(AnchorKind::West) => Side::Left,
        Some(AnchorKind::NorthEast) => {
            if horizontal {
                Side::Right
            } else {
                Side::Top
            }
        }
        Some(AnchorKind::NorthWest) => {
            if horizontal {
                Side::Left
            } else {
                Side::Top
            }
        }
        Some(AnchorKind::SouthEast) => {
            if horizontal {
                Side::Right
            } else {
                Side::Bottom
            }
        }
        Some(AnchorKind::SouthWest) => {
            if horizontal {
                Side::Left
            } else {
                Side::Bottom
            }
        }
        _ if horizontal => {
            if to.x >= from.x {
                Side::Right
            } else {
                Side::Left
            }
        }
        _ => {
            if to.y >= from.y {
                Side::Bottom
            } else {
                Side::Top
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_graph::route_connector;

    fn free(start: GraphPoint, end: GraphPoint) -> ConnectorSpec {
        ConnectorSpec {
            start: Endpoint::Free(start),
            end: Endpoint::Free(end),
            kind: ConnectorKind::Orthogonal,
        }
    }

    #[test]
    fn large_absolute_coordinates_are_routed_without_endpoint_drift() {
        let start = GraphPoint::new(16_777_217.0, 16_777_217.0);
        let end = GraphPoint::new(start.x + 400.0, start.y);
        let connector = free(start, end);
        let legacy = route_connector(&connector, |_| None).unwrap();
        let obstacle = GraphRect::new(start.x + 150.0, start.y - 50.0, 100.0, 100.0);
        let routed = avoid_obstacles(&connector, legacy.clone(), [obstacle].into_iter());
        assert_ne!(routed, legacy);
        let RoutedPath::Polyline(points) = routed else {
            panic!()
        };
        assert_eq!(points.first(), Some(&start));
        assert_eq!(points.last(), Some(&end));
        assert!(!crosses(&points, obstacle));
    }

    #[test]
    fn unrepresentable_spans_and_large_obstacle_sets_keep_the_existing_route() {
        let connector = free(
            GraphPoint::new(0.0, 0.0),
            GraphPoint::new(16_777_217.0, 101.0),
        );
        let legacy = route_connector(&connector, |_| None).unwrap();
        let obstacle = GraphRect::new(100.0, -10.0, 100.0, 200.0);
        assert_eq!(
            avoid_obstacles(&connector, legacy.clone(), [obstacle].into_iter()),
            legacy
        );
        let connector = free(GraphPoint::new(0.0, 0.0), GraphPoint::new(400.0, 0.0));
        let legacy = route_connector(&connector, |_| None).unwrap();
        let obstacles =
            (0..=MAX_OBSTACLES).map(|i| GraphRect::new(100.0, i as f64 - 50.0, 100.0, 100.0));
        assert_eq!(
            avoid_obstacles(&connector, legacy.clone(), obstacles),
            legacy
        );
    }

    #[test]
    fn a_blocked_port_does_not_accept_an_invalid_upstream_fallback() {
        let connector = free(GraphPoint::new(0.0, 0.0), GraphPoint::new(400.0, 0.0));
        let legacy = route_connector(&connector, |_| None).unwrap();
        let obstacle = GraphRect::new(5.0, -20.0, 10.0, 40.0);
        let routed = avoid_obstacles(&connector, legacy.clone(), [obstacle].into_iter());
        assert_eq!(routed, legacy);
    }

    #[test]
    fn same_node_edges_keep_their_original_attachment_points() {
        let bounds = |_: &str| Some(GraphRect::new(0.0, 0.0, 100.0, 60.0));
        let connector = ConnectorSpec {
            start: Endpoint::Bound {
                node_id: "same-node".into(),
                anchor: AnchorKind::West,
                offset: GraphPoint::new(0.0, 0.0),
            },
            end: Endpoint::Bound {
                node_id: "same-node".into(),
                anchor: AnchorKind::East,
                offset: GraphPoint::new(0.0, 0.0),
            },
            kind: ConnectorKind::Orthogonal,
        };
        let legacy = route_connector(&connector, bounds).unwrap();
        let RoutedPath::Polyline(points) = avoid_obstacles(
            &connector,
            legacy,
            [bounds("same-node").unwrap()].into_iter(),
        ) else {
            panic!()
        };
        assert_eq!(points.first(), Some(&GraphPoint::new(0.0, 30.0)));
        assert_eq!(points.last(), Some(&GraphPoint::new(100.0, 30.0)));
    }

    #[test]
    fn every_existing_anchor_combination_preserves_its_endpoints() {
        use mesh_graph::resolve_endpoint;
        let anchors = [
            AnchorKind::Center,
            AnchorKind::North,
            AnchorKind::South,
            AnchorKind::East,
            AnchorKind::West,
            AnchorKind::NorthEast,
            AnchorKind::NorthWest,
            AnchorKind::SouthEast,
            AnchorKind::SouthWest,
        ];
        let bounds = |id: &str| {
            Some(if id == "left" {
                GraphRect::new(-100.0, -60.0, 100.0, 60.0)
            } else {
                GraphRect::new(300.0, -60.0, 100.0, 60.0)
            })
        };
        for start in anchors {
            for end in anchors {
                let connector = ConnectorSpec {
                    start: Endpoint::Bound {
                        node_id: "left".into(),
                        anchor: start,
                        offset: GraphPoint::new(0.0, 0.0),
                    },
                    end: Endpoint::Bound {
                        node_id: "right".into(),
                        anchor: end,
                        offset: GraphPoint::new(0.0, 0.0),
                    },
                    kind: ConnectorKind::Orthogonal,
                };
                let legacy = route_connector(&connector, bounds).unwrap();
                let obstacles = [
                    bounds("left").unwrap(),
                    bounds("right").unwrap(),
                    GraphRect::new(100.0, -100.0, 100.0, 150.0),
                ];
                let RoutedPath::Polyline(points) =
                    avoid_obstacles(&connector, legacy, obstacles.into_iter())
                else {
                    panic!()
                };
                assert_eq!(
                    points.first(),
                    resolve_endpoint(&connector.start, bounds).as_ref()
                );
                assert_eq!(
                    points.last(),
                    resolve_endpoint(&connector.end, bounds).as_ref()
                );
            }
        }
    }
}
