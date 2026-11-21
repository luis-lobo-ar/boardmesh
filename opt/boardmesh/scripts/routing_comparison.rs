use mesh_graph::{
    AnchorKind as A, ConnectorKind, ConnectorSpec, Endpoint, GraphPoint as P, GraphRect as R,
    RoutedPath, resolve_endpoint, route_connector,
};
use ortho_router::{Connector, Options, Port, Rect, Router, Side};
use serde_json::json;

fn side(anchor: Option<A>, from: P, to: P) -> Side {
    let horizontal = (to.x - from.x).abs() >= (to.y - from.y).abs();
    match anchor {
        Some(A::North) => Side::Top,
        Some(A::South) => Side::Bottom,
        Some(A::East) => Side::Right,
        Some(A::West) => Side::Left,
        Some(A::NorthEast) => {
            if horizontal {
                Side::Right
            } else {
                Side::Top
            }
        }
        Some(A::NorthWest) => {
            if horizontal {
                Side::Left
            } else {
                Side::Top
            }
        }
        Some(A::SouthEast) => {
            if horizontal {
                Side::Right
            } else {
                Side::Bottom
            }
        }
        Some(A::SouthWest) => {
            if horizontal {
                Side::Left
            } else {
                Side::Bottom
            }
        }
        _ => {
            if horizontal {
                if to.x >= from.x {
                    Side::Right
                } else {
                    Side::Left
                }
            } else if to.y >= from.y {
                Side::Bottom
            } else {
                Side::Top
            }
        }
    }
}
fn anchor(e: &Endpoint) -> Option<A> {
    match e {
        Endpoint::Bound { anchor, .. } => Some(*anchor),
        _ => None,
    }
}
fn points(path: RoutedPath) -> Vec<P> {
    match path {
        RoutedPath::Polyline(p) => p,
        _ => panic!("only orthogonal fixtures"),
    }
}
fn crosses(path: &[P], r: R) -> bool {
    path.windows(2).any(|p| {
        let (a, b) = (p[0], p[1]);
        if a.y == b.y {
            a.y > r.y && a.y < r.y + r.height && a.x.min(b.x) < r.x + r.width && a.x.max(b.x) > r.x
        } else if a.x == b.x {
            a.x > r.x && a.x < r.x + r.width && a.y.min(b.y) < r.y + r.height && a.y.max(b.y) > r.y
        } else {
            true
        }
    })
}
fn candidate(
    spec: &ConnectorSpec,
    nodes: &[(&str, R)],
    obstacles: &[R],
    normalize: bool,
) -> (bool, Vec<P>) {
    let lookup = |id: &str| nodes.iter().find(|n| n.0 == id).map(|n| n.1);
    let start = resolve_endpoint(&spec.start, lookup).unwrap();
    let end = resolve_endpoint(&spec.end, lookup).unwrap();
    let origin = if normalize { start } else { P::new(0., 0.) };
    let mut router = Router::new(Options::default()).with_obstacles(obstacles.iter().map(|r| {
        Rect::from_min_size(
            [(r.x - origin.x) as f32, (r.y - origin.y) as f32],
            r.width as f32,
            r.height as f32,
        )
    }));
    let wire = Connector::new(
        Port::new(
            [(start.x - origin.x) as f32, (start.y - origin.y) as f32],
            side(anchor(&spec.start), start, end),
        ),
        Port::new(
            [(end.x - origin.x) as f32, (end.y - origin.y) as f32],
            side(anchor(&spec.end), end, start),
        ),
    );
    let result = router.route(&wire);
    (
        result.routed,
        result
            .points
            .iter()
            .map(|p| P::new(p.x as f64 + origin.x, p.y as f64 + origin.y))
            .collect(),
    )
}
fn main() {
    let mut rows = Vec::new();
    let nodes = [
        ("left", R::new(0., 0., 100., 60.)),
        ("right", R::new(400., 0., 100., 60.)),
    ];
    let bound = |id: &str, a: A, offset: P| Endpoint::Bound {
        node_id: id.into(),
        anchor: a,
        offset,
    };
    let fixtures = vec![
        (
            "clear",
            bound("left", A::East, P::new(0., 0.)),
            bound("right", A::West, P::new(0., 0.)),
            vec![nodes[0].1, nodes[1].1],
        ),
        (
            "blocked",
            bound("left", A::East, P::new(0., 0.)),
            bound("right", A::West, P::new(0., 0.)),
            vec![nodes[0].1, nodes[1].1, R::new(200., -40., 100., 140.)],
        ),
        (
            "vertical-free",
            Endpoint::Free(P::new(0., 0.)),
            Endpoint::Free(P::new(0., 400.)),
            vec![R::new(-40., 150., 80., 100.)],
        ),
        (
            "negative",
            Endpoint::Free(P::new(-400., -100.)),
            Endpoint::Free(P::new(0., -100.)),
            vec![R::new(-250., -150., 80., 100.)],
        ),
        (
            "precision",
            Endpoint::Free(P::new(16777217., 16777217.)),
            Endpoint::Free(P::new(16777417., 16777317.)),
            vec![],
        ),
        (
            "wide-precision",
            Endpoint::Free(P::new(0., 0.)),
            Endpoint::Free(P::new(16777217., 101.)),
            vec![],
        ),
        (
            "same-point",
            Endpoint::Free(P::new(10., 10.)),
            Endpoint::Free(P::new(10., 10.)),
            vec![],
        ),
        (
            "offset",
            bound("left", A::East, P::new(8., -4.)),
            bound("right", A::West, P::new(-8., 4.)),
            vec![R::new(200., -40., 100., 140.)],
        ),
    ];
    for (name, start, end, obstacles) in fixtures {
        let spec = ConnectorSpec {
            start,
            end,
            kind: ConnectorKind::Orthogonal,
        };
        let lookup = |id: &str| nodes.iter().find(|n| n.0 == id).map(|n| n.1);
        let old = points(route_connector(&spec, lookup).unwrap());
        let (a, p) = candidate(&spec, &nodes, &obstacles, false);
        let (b, q) = candidate(&spec, &nodes, &obstacles, true);
        rows.push(json!({"name":name,"old":old,"candidate":p,"normalized":q,"routed":a,"normalized_routed":b,"old_crossings":obstacles.iter().filter(|r|crosses(&old,**r)).count(),"candidate_crossings":obstacles.iter().filter(|r|crosses(&p,**r)).count(),"normalized_exact_endpoints":q.first()==old.first() && q.last()==old.last(),"raw_exact_endpoints":p.first()==old.first() && p.last()==old.last()}));
    }
    for from in [
        A::North,
        A::South,
        A::East,
        A::West,
        A::NorthEast,
        A::NorthWest,
        A::SouthEast,
        A::SouthWest,
        A::Center,
    ] {
        for to in [
            A::North,
            A::South,
            A::East,
            A::West,
            A::NorthEast,
            A::NorthWest,
            A::SouthEast,
            A::SouthWest,
            A::Center,
        ] {
            let spec = ConnectorSpec {
                start: bound("left", from, P::new(0., 0.)),
                end: bound("right", to, P::new(0., 0.)),
                kind: ConnectorKind::Orthogonal,
            };
            let lookup = |id: &str| nodes.iter().find(|n| n.0 == id).map(|n| n.1);
            let old = points(route_connector(&spec, lookup).unwrap());
            let (routed, p) = candidate(&spec, &nodes, &[], true);
            rows.push(json!({"name":format!("anchors-{from:?}-{to:?}"),"normalized_routed":routed,"normalized_exact_endpoints":p.first()==old.first() && p.last()==old.last()}));
        }
    }
    println!("{}", serde_json::to_string_pretty(&rows).unwrap());
}
