use boardmesh::{CanvasPoint, ResolvedConnectorPath, UniverseDocument};

fn legacy_board() -> UniverseDocument {
    serde_json::from_str(include_str!("fixtures/legacy-orthogonal-board.json")).unwrap()
}

#[test]
fn old_board_avoids_a_blocker_without_changing_its_document_or_endpoints() {
    let board = legacy_board();
    let before = serde_json::to_value(&board).unwrap();
    let ResolvedConnectorPath::Polyline(points) = board
        .scene
        .resolved_connector_path("existing-connection")
        .unwrap()
    else {
        panic!("orthogonal connector must remain a polyline")
    };
    assert_eq!(points.first(), Some(&CanvasPoint::new(100, 30)));
    assert_eq!(points.last(), Some(&CanvasPoint::new(400, 30)));
    for pair in points.windows(2) {
        assert!(pair[0].x == pair[1].x || pair[0].y == pair[1].y);
        let crosses = if pair[0].y == pair[1].y {
            pair[0].y > -40
                && pair[0].y < 100
                && pair[0].x.min(pair[1].x) < 300
                && pair[0].x.max(pair[1].x) > 200
        } else {
            pair[0].x > 200
                && pair[0].x < 300
                && pair[0].y.min(pair[1].y) < 100
                && pair[0].y.max(pair[1].y) > -40
        };
        assert!(
            !crosses,
            "connector crosses the existing blocker: {points:?}"
        );
    }
    let midpoint = points
        .windows(2)
        .map(|pair| CanvasPoint::new((pair[0].x + pair[1].x) / 2, (pair[0].y + pair[1].y) / 2))
        .find(|point| point.y < -40 || point.y > 100)
        .unwrap();
    assert_eq!(
        board.scene.topmost_object_at(midpoint).unwrap().id,
        "existing-connection"
    );
    assert_eq!(serde_json::to_value(&board).unwrap(), before);
}

#[test]
fn clear_existing_board_keeps_its_exact_route() {
    let mut board = legacy_board();
    board
        .scene
        .objects
        .iter_mut()
        .find(|object| object.id == "existing-blocker")
        .unwrap()
        .visible = false;
    assert_eq!(
        board.scene.resolved_connector_path("existing-connection"),
        Some(ResolvedConnectorPath::Polyline(vec![
            CanvasPoint::new(100, 30),
            CanvasPoint::new(250, 30),
            CanvasPoint::new(250, 30),
            CanvasPoint::new(400, 30)
        ]))
    );
}

#[test]
fn existing_straight_and_spline_styles_keep_their_geometry() {
    use boardmesh::{ObjectKind, SceneConnectorKind};
    for kind in [SceneConnectorKind::Straight, SceneConnectorKind::Spline] {
        let mut board = legacy_board();
        let ObjectKind::Connector { connector } = &mut board.scene.objects[3].kind else {
            panic!()
        };
        connector.kind = kind;
        match board
            .scene
            .resolved_connector_path("existing-connection")
            .unwrap()
        {
            ResolvedConnectorPath::Polyline(points) => {
                assert_eq!(kind, SceneConnectorKind::Straight);
                assert_eq!(
                    points,
                    vec![CanvasPoint::new(100, 30), CanvasPoint::new(400, 30)]
                );
            }
            ResolvedConnectorPath::CubicBezier(segments) => {
                assert_eq!(kind, SceneConnectorKind::Spline);
                assert_eq!(segments[0].start, CanvasPoint::new(100, 30));
                assert_eq!(segments[0].control_start, CanvasPoint::new(250, 30));
                assert_eq!(segments[0].control_end, CanvasPoint::new(250, 30));
                assert_eq!(segments[0].end, CanvasPoint::new(400, 30));
            }
        }
    }
}
