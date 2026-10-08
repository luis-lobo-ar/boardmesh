use crate::universe::UniverseDocument;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SelectionContextKind {
    None,
    SingleObject,
    MultiObject,
    Canvas,
    Mixed,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ObjectSelectionContext {
    pub object_id: String,
    pub object_kind: String,
    pub semantic_kind: String,
    pub canvas_id: Option<String>,
    pub connected_object_ids: Vec<String>,
    pub nearby_object_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SelectionContext {
    pub kind: SelectionContextKind,
    pub active_canvas_id: Option<String>,
    pub selected_canvas_ids: Vec<String>,
    pub primary_object_id: Option<String>,
    pub selected_object_ids: Vec<String>,
    pub selected_objects: Vec<ObjectSelectionContext>,
}

impl SelectionContext {
    pub fn from_universe(universe: &UniverseDocument) -> Self {
        let selected_object_ids = universe.scene.selected_object_ids.clone();
        let selected_canvas_ids = universe.selected_canvas_ids.clone();
        let primary_object_id = selected_object_ids.first().cloned();
        let kind = match (selected_object_ids.len(), selected_canvas_ids.is_empty()) {
            (0, true) => SelectionContextKind::None,
            (0, false) => SelectionContextKind::Canvas,
            (1, true) => SelectionContextKind::SingleObject,
            (_, true) => SelectionContextKind::MultiObject,
            _ => SelectionContextKind::Mixed,
        };

        let selected_objects = selected_object_ids
            .iter()
            .filter_map(|object_id| {
                let object = universe
                    .scene
                    .objects
                    .iter()
                    .find(|object| object.id == *object_id)?;
                Some(ObjectSelectionContext {
                    object_id: object.id.clone(),
                    object_kind: object_kind_name(object).to_string(),
                    semantic_kind: semantic_kind_name(object).to_string(),
                    canvas_id: universe.object_canvas_id(&object.id).map(str::to_string),
                    connected_object_ids: connected_object_ids(universe, &object.id),
                    nearby_object_ids: nearby_object_ids(universe, &object.id),
                })
            })
            .collect();

        Self {
            kind,
            active_canvas_id: universe.active_canvas_id.clone(),
            selected_canvas_ids,
            primary_object_id,
            selected_object_ids,
            selected_objects,
        }
    }
}

fn object_kind_name(object: &crate::scene::DrawableObject) -> &'static str {
    match &object.kind {
        crate::scene::ObjectKind::Line { .. } => "line",
        crate::scene::ObjectKind::Connector { .. } => "connector",
        crate::scene::ObjectKind::Link { .. } => "link",
        crate::scene::ObjectKind::Rectangle { .. } => "rectangle",
        crate::scene::ObjectKind::Circle { .. } => "circle",
        crate::scene::ObjectKind::Triangle { .. } => "triangle",
        crate::scene::ObjectKind::Polygon { .. } => "polygon",
        crate::scene::ObjectKind::Stroke { .. } => "stroke",
        crate::scene::ObjectKind::Text { .. } => "text",
    }
}

fn semantic_kind_name(object: &crate::scene::DrawableObject) -> &'static str {
    object_kind_name(object)
}

fn connected_object_ids(universe: &UniverseDocument, object_id: &str) -> Vec<String> {
    let mut connected = Vec::new();

    for object in &universe.scene.objects {
        match &object.kind {
            crate::scene::ObjectKind::Connector { connector } => {
                collect_connected_ids(
                    object_id,
                    endpoint_object_id(&connector.start),
                    endpoint_object_id(&connector.end),
                    &mut connected,
                );
            }
            crate::scene::ObjectKind::Link { link } => {
                collect_connected_ids(
                    object_id,
                    endpoint_object_id(&link.source.endpoint),
                    endpoint_object_id(&link.target.endpoint),
                    &mut connected,
                );
            }
            _ => {}
        }
    }

    connected
}

fn collect_connected_ids(
    object_id: &str,
    start: Option<&str>,
    end: Option<&str>,
    connected: &mut Vec<String>,
) {
    match (start, end) {
        (Some(start), Some(end)) if object_id == start && start != end => {
            push_unique(connected, end.to_string());
        }
        (Some(start), Some(end)) if object_id == end && start != end => {
            push_unique(connected, start.to_string());
        }
        (Some(start), Some(end)) if object_id != start && object_id != end => {}
        (Some(start), Some(end)) => {
            push_unique(connected, start.to_string());
            push_unique(connected, end.to_string());
        }
        _ => {}
    }
}

fn endpoint_object_id(endpoint: &crate::scene::ConnectorEndpoint) -> Option<&str> {
    match endpoint {
        crate::scene::ConnectorEndpoint::Free { .. } => None,
        crate::scene::ConnectorEndpoint::Bound { object_id, .. } => Some(object_id.as_str()),
    }
}

fn nearby_object_ids(universe: &UniverseDocument, object_id: &str) -> Vec<String> {
    let Some(primary_bounds) = object_bounds(universe, object_id) else {
        return Vec::new();
    };
    let nearby_zone = primary_bounds.inflate(80);
    let mut nearby = Vec::new();

    for object in &universe.scene.objects {
        if object.id == object_id || !object.visible {
            continue;
        }
        if let Some(bounds) = object_bounds(universe, &object.id)
            && bounds.intersects(nearby_zone)
        {
            push_unique(&mut nearby, object.id.clone());
        }
    }

    nearby
}

fn object_bounds(universe: &UniverseDocument, object_id: &str) -> Option<crate::scene::SceneRect> {
    let object = universe
        .scene
        .objects
        .iter()
        .find(|object| object.id == object_id)?;
    match &object.kind {
        crate::scene::ObjectKind::Connector { .. } | crate::scene::ObjectKind::Link { .. } => {
            universe
                .scene
                .resolved_connector_path(object_id)
                .map(connector_path_bounds)
        }
        _ => Some(object.bounds()),
    }
}

fn connector_path_bounds(path: crate::scene::ResolvedConnectorPath) -> crate::scene::SceneRect {
    match path {
        crate::scene::ResolvedConnectorPath::Polyline(points) => points_bounds(&points),
        crate::scene::ResolvedConnectorPath::CubicBezier(segments) => {
            let mut points = Vec::new();
            for segment in segments {
                points.push(segment.start);
                points.push(segment.control_start);
                points.push(segment.control_end);
                points.push(segment.end);
            }
            points_bounds(&points)
        }
    }
}

fn points_bounds(points: &[crate::scene::CanvasPoint]) -> crate::scene::SceneRect {
    let mut left = points[0].x;
    let mut top = points[0].y;
    let mut right = points[0].x;
    let mut bottom = points[0].y;

    for point in points.iter().skip(1) {
        left = left.min(point.x);
        top = top.min(point.y);
        right = right.max(point.x);
        bottom = bottom.max(point.y);
    }

    crate::scene::SceneRect::new(left, top, right - left, bottom - top)
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{
        CanvasPoint, ConnectorAnchor, ConnectorEndpoint, LinkEnd, LinkObject,
        LinkRoutingPreference, SceneRect, SceneStroke,
    };

    #[test]
    fn derives_context_for_a_selected_object_with_canvas_links_and_nearby_objects() {
        let mut universe = UniverseDocument::default();
        let canvas_id = universe.create_canvas("Review", SceneRect::new(0, 0, 320, 220));
        let source_id = universe.scene.add_rectangle(
            SceneRect::new(20, 20, 80, 50),
            SceneStroke::default(),
            None,
        );
        let nearby_id =
            universe
                .scene
                .add_circle(CanvasPoint::new(150, 50), 24, SceneStroke::default(), None);
        let target_id = universe.scene.add_rectangle(
            SceneRect::new(360, 40, 90, 60),
            SceneStroke::default(),
            None,
        );
        let _link_id = universe.scene.add_link(
            LinkObject {
                source: LinkEnd {
                    endpoint: ConnectorEndpoint::Bound {
                        object_id: source_id.clone(),
                        anchor: ConnectorAnchor::East,
                        offset: CanvasPoint::new(0, 0),
                    },
                    label: Some("from".to_string()),
                },
                target: LinkEnd {
                    endpoint: ConnectorEndpoint::Bound {
                        object_id: target_id.clone(),
                        anchor: ConnectorAnchor::West,
                        offset: CanvasPoint::new(0, 0),
                    },
                    label: Some("to".to_string()),
                },
                routing: LinkRoutingPreference::Straight,
                label: Some("references".to_string()),
            },
            SceneStroke::default(),
        );
        assert!(universe.assign_object_to_canvas(&source_id, &canvas_id));
        assert!(universe.scene.select_objects(vec![source_id.clone()]));

        let context = SelectionContext::from_universe(&universe);

        assert_eq!(context.kind, SelectionContextKind::SingleObject);
        assert_eq!(
            context.primary_object_id.as_deref(),
            Some(source_id.as_str())
        );
        assert_eq!(context.selected_object_ids, vec![source_id.clone()]);
        assert_eq!(
            context.active_canvas_id.as_deref(),
            Some(canvas_id.as_str())
        );
        assert_eq!(context.selected_objects.len(), 1);
        let primary = &context.selected_objects[0];
        assert_eq!(primary.object_id, source_id);
        assert_eq!(primary.object_kind, "rectangle");
        assert_eq!(primary.semantic_kind, "rectangle");
        assert_eq!(primary.canvas_id.as_deref(), Some(canvas_id.as_str()));
        assert!(primary.connected_object_ids.contains(&target_id));
        assert!(primary.nearby_object_ids.contains(&nearby_id));
    }
}
