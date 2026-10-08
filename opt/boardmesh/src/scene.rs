use mesh_graph::{
    AnchorKind as GraphAnchorKind, ConnectorKind as GraphConnectorKind,
    ConnectorSpec as GraphConnectorSpec, CubicBezierSegment as GraphCubicBezierSegment,
    Endpoint as GraphEndpoint, GraphPoint, GraphRect, RoutedPath as GraphRoutedPath,
    route_connector as route_graph_connector, route_hits_point as graph_route_hits_point,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(try_from = "CanvasPointInput")]
pub struct CanvasPoint {
    pub x: i32,
    pub y: i32,
}

impl CanvasPoint {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn translate(self, dx: i32, dy: i32) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(try_from = "SceneRectInput")]
pub struct SceneRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl SceneRect {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn from_points(start: CanvasPoint, end: CanvasPoint) -> Self {
        let left = start.x.min(end.x);
        let top = start.y.min(end.y);
        let right = start.x.max(end.x);
        let bottom = start.y.max(end.y);
        Self::new(left, top, right - left, bottom - top)
    }

    pub fn contains(&self, point: CanvasPoint) -> bool {
        point.x >= self.x
            && point.x <= self.x + self.width
            && point.y >= self.y
            && point.y <= self.y + self.height
    }

    pub fn intersects(&self, other: SceneRect) -> bool {
        let self_right = self.x + self.width;
        let self_bottom = self.y + self.height;
        let other_right = other.x + other.width;
        let other_bottom = other.y + other.height;

        self.x <= other_right
            && self_right >= other.x
            && self.y <= other_bottom
            && self_bottom >= other.y
    }

    pub fn union(self, other: SceneRect) -> SceneRect {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);
        SceneRect::new(left, top, right - left, bottom - top)
    }

    pub fn inflate(self, amount: i32) -> SceneRect {
        SceneRect::new(
            self.x - amount,
            self.y - amount,
            self.width + amount * 2,
            self.height + amount * 2,
        )
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
enum CanvasPointInput {
    Object { x: i32, y: i32 },
    Pair([i32; 2]),
    String(String),
}

impl TryFrom<CanvasPointInput> for CanvasPoint {
    type Error = String;

    fn try_from(value: CanvasPointInput) -> Result<Self, Self::Error> {
        match value {
            CanvasPointInput::Object { x, y } => Ok(Self::new(x, y)),
            CanvasPointInput::Pair([x, y]) => Ok(Self::new(x, y)),
            CanvasPointInput::String(value) => parse_canvas_point(&value),
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
enum SceneRectInput {
    Object {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
    Quad([i32; 4]),
    String(String),
}

impl TryFrom<SceneRectInput> for SceneRect {
    type Error = String;

    fn try_from(value: SceneRectInput) -> Result<Self, Self::Error> {
        match value {
            SceneRectInput::Object {
                x,
                y,
                width,
                height,
            } => Ok(Self::new(x, y, width, height)),
            SceneRectInput::Quad([x, y, width, height]) => Ok(Self::new(x, y, width, height)),
            SceneRectInput::String(value) => parse_scene_rect(&value),
        }
    }
}

fn parse_canvas_point(value: &str) -> Result<CanvasPoint, String> {
    let parts = value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() != 2 {
        return Err(format!(
            "invalid CanvasPoint string {value:?}; expected \"x,y\""
        ));
    }
    let x = parts[0]
        .parse::<i32>()
        .map_err(|_| format!("invalid CanvasPoint x component in {value:?}"))?;
    let y = parts[1]
        .parse::<i32>()
        .map_err(|_| format!("invalid CanvasPoint y component in {value:?}"))?;
    Ok(CanvasPoint::new(x, y))
}

fn parse_scene_rect(value: &str) -> Result<SceneRect, String> {
    let parts = value
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() != 4 {
        return Err(format!(
            "invalid SceneRect string {value:?}; expected \"x,y,width,height\""
        ));
    }
    let x = parts[0]
        .parse::<i32>()
        .map_err(|_| format!("invalid SceneRect x component in {value:?}"))?;
    let y = parts[1]
        .parse::<i32>()
        .map_err(|_| format!("invalid SceneRect y component in {value:?}"))?;
    let width = parts[2]
        .parse::<i32>()
        .map_err(|_| format!("invalid SceneRect width component in {value:?}"))?;
    let height = parts[3]
        .parse::<i32>()
        .map_err(|_| format!("invalid SceneRect height component in {value:?}"))?;
    Ok(SceneRect::new(x, y, width, height))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SceneColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    #[serde(default = "opaque_alpha")]
    pub a: u8,
}

impl SceneColor {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn transparent() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }
    }
}

impl Default for SceneColor {
    fn default() -> Self {
        Self::rgb(24, 24, 27)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SceneStroke {
    pub color: SceneColor,
    #[serde(default = "default_stroke_width")]
    pub width: i32,
}

impl Default for SceneStroke {
    fn default() -> Self {
        Self {
            color: SceneColor::default(),
            width: default_stroke_width(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TextObject {
    pub position: CanvasPoint,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TriangleObject {
    pub points: [CanvasPoint; 3],
}

impl TriangleObject {
    pub const fn new(points: [CanvasPoint; 3]) -> Self {
        Self { points }
    }

    pub fn from_rect(rect: SceneRect) -> Self {
        Self::new([
            CanvasPoint::new(rect.x + rect.width / 2, rect.y),
            CanvasPoint::new(rect.x, rect.y + rect.height),
            CanvasPoint::new(rect.x + rect.width, rect.y + rect.height),
        ])
    }

    pub fn as_points(&self) -> &[CanvasPoint] {
        &self.points
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorAnchor {
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConnectorEndpoint {
    Free {
        point: CanvasPoint,
    },
    Bound {
        object_id: String,
        anchor: ConnectorAnchor,
        offset: CanvasPoint,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SceneConnectorKind {
    Straight,
    Orthogonal,
    Spline,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ConnectorSpec {
    pub start: ConnectorEndpoint,
    pub end: ConnectorEndpoint,
    pub kind: SceneConnectorKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LinkEnd {
    pub endpoint: ConnectorEndpoint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum LinkRoutingPreference {
    #[default]
    Straight,
    Orthogonal,
    Spline,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LinkObject {
    pub source: LinkEnd,
    pub target: LinkEnd,
    #[serde(default)]
    pub routing: LinkRoutingPreference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl LinkObject {
    pub fn to_connector_spec(&self) -> ConnectorSpec {
        ConnectorSpec {
            start: self.source.endpoint.clone(),
            end: self.target.endpoint.clone(),
            kind: self.routing.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedCubicBezierSegment {
    pub start: CanvasPoint,
    pub control_start: CanvasPoint,
    pub control_end: CanvasPoint,
    pub end: CanvasPoint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedConnectorPath {
    Polyline(Vec<CanvasPoint>),
    CubicBezier(Vec<ResolvedCubicBezierSegment>),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ObjectKind {
    Line {
        start: CanvasPoint,
        end: CanvasPoint,
    },
    Connector {
        connector: ConnectorSpec,
    },
    Link {
        link: LinkObject,
    },
    Rectangle {
        rect: SceneRect,
    },
    Circle {
        center: CanvasPoint,
        radius: i32,
    },
    Triangle {
        triangle: TriangleObject,
    },
    Polygon {
        points: Vec<CanvasPoint>,
    },
    Stroke {
        points: Vec<CanvasPoint>,
    },
    Text {
        text: TextObject,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DrawableObject {
    pub id: String,
    pub kind: ObjectKind,
    #[serde(default)]
    pub stroke: SceneStroke,
    #[serde(default)]
    pub fill: Option<SceneColor>,
    #[serde(default = "default_object_visible")]
    pub visible: bool,
    #[serde(default = "default_object_locked")]
    pub locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SceneDocument {
    #[serde(default = "default_scene_version")]
    pub version: u32,
    #[serde(default = "default_scene_background")]
    pub background: SceneColor,
    #[serde(default)]
    pub selected_object_ids: Vec<String>,
    #[serde(default)]
    pub objects: Vec<DrawableObject>,
}

impl Default for SceneDocument {
    fn default() -> Self {
        Self {
            version: default_scene_version(),
            background: default_scene_background(),
            selected_object_ids: Vec::new(),
            objects: Vec::new(),
        }
    }
}

impl SceneDocument {
    pub fn add_line(
        &mut self,
        start: CanvasPoint,
        end: CanvasPoint,
        stroke: SceneStroke,
    ) -> String {
        self.push(ObjectKind::Line { start, end }, stroke, None)
    }

    pub fn add_connector(&mut self, connector: ConnectorSpec, stroke: SceneStroke) -> String {
        self.push(ObjectKind::Connector { connector }, stroke, None)
    }

    pub fn add_link(&mut self, link: LinkObject, stroke: SceneStroke) -> String {
        self.push(ObjectKind::Link { link }, stroke, None)
    }

    pub fn add_rectangle(
        &mut self,
        rect: SceneRect,
        stroke: SceneStroke,
        fill: Option<SceneColor>,
    ) -> String {
        self.push(ObjectKind::Rectangle { rect }, stroke, fill)
    }

    pub fn add_circle(
        &mut self,
        center: CanvasPoint,
        radius: i32,
        stroke: SceneStroke,
        fill: Option<SceneColor>,
    ) -> String {
        self.push(
            ObjectKind::Circle {
                center,
                radius: radius.max(1),
            },
            stroke,
            fill,
        )
    }

    pub fn add_triangle(
        &mut self,
        triangle: TriangleObject,
        stroke: SceneStroke,
        fill: Option<SceneColor>,
    ) -> String {
        self.push(ObjectKind::Triangle { triangle }, stroke, fill)
    }

    pub fn add_polygon(
        &mut self,
        points: Vec<CanvasPoint>,
        stroke: SceneStroke,
        fill: Option<SceneColor>,
    ) -> Option<String> {
        if points.len() < 3 {
            return None;
        }
        Some(self.push(ObjectKind::Polygon { points }, stroke, fill))
    }

    pub fn add_stroke(&mut self, points: Vec<CanvasPoint>, stroke: SceneStroke) -> Option<String> {
        if points.len() < 2 {
            return None;
        }
        Some(self.push(ObjectKind::Stroke { points }, stroke, None))
    }

    pub fn add_text(&mut self, text: TextObject, color: SceneColor) -> String {
        self.push(
            ObjectKind::Text { text },
            SceneStroke { color, width: 1 },
            None,
        )
    }

    pub fn clear(&mut self) {
        self.objects.clear();
        self.selected_object_ids.clear();
    }

    pub fn delete_object(&mut self, object_id: &str) -> bool {
        let len_before = self.objects.len();
        self.objects.retain(|item| item.id != object_id);
        let deleted = len_before != self.objects.len();
        if deleted {
            self.selected_object_ids
                .retain(|selected_id| selected_id != object_id);
        }
        deleted
    }

    pub fn delete_objects(&mut self, object_ids: &[String]) -> usize {
        let len_before = self.objects.len();
        self.objects
            .retain(|item| !object_ids.iter().any(|object_id| object_id == &item.id));
        self.selected_object_ids
            .retain(|selected_id| !object_ids.iter().any(|object_id| object_id == selected_id));
        len_before.saturating_sub(self.objects.len())
    }

    pub fn duplicate_object(&mut self, object_id: &str, dx: i32, dy: i32) -> Option<String> {
        let source = self
            .objects
            .iter()
            .find(|item| item.id == object_id)?
            .clone();
        let new_id = format!("obj-{}", Uuid::new_v4());
        let mut duplicate = source;
        duplicate.id = new_id.clone();
        duplicate.kind.translate(dx, dy);
        self.objects.push(duplicate);
        Some(new_id)
    }

    pub fn set_object_visibility(&mut self, object_id: &str, visible: bool) -> bool {
        let Some(object) = self.objects.iter_mut().find(|item| item.id == object_id) else {
            return false;
        };
        object.visible = visible;
        if !visible {
            self.selected_object_ids
                .retain(|selected_id| selected_id != object_id);
        }
        true
    }

    pub fn set_object_locked(&mut self, object_id: &str, locked: bool) -> bool {
        let Some(object) = self.objects.iter_mut().find(|item| item.id == object_id) else {
            return false;
        };
        object.locked = locked;
        true
    }

    pub fn move_object_to_front(&mut self, object_id: &str) -> bool {
        let Some(index) = self.objects.iter().position(|item| item.id == object_id) else {
            return false;
        };
        if index + 1 == self.objects.len() {
            return true;
        }
        let object = self.objects.remove(index);
        self.objects.push(object);
        true
    }

    pub fn move_object_to_back(&mut self, object_id: &str) -> bool {
        let Some(index) = self.objects.iter().position(|item| item.id == object_id) else {
            return false;
        };
        if index == 0 {
            return true;
        }
        let object = self.objects.remove(index);
        self.objects.insert(0, object);
        true
    }

    pub fn move_object_forward(&mut self, object_id: &str) -> bool {
        let Some(index) = self.objects.iter().position(|item| item.id == object_id) else {
            return false;
        };
        if index + 1 >= self.objects.len() {
            return true;
        }
        self.objects.swap(index, index + 1);
        true
    }

    pub fn move_object_backward(&mut self, object_id: &str) -> bool {
        let Some(index) = self.objects.iter().position(|item| item.id == object_id) else {
            return false;
        };
        if index == 0 {
            return true;
        }
        self.objects.swap(index, index - 1);
        true
    }

    pub fn fill_object(&mut self, object_id: &str, color: SceneColor) -> bool {
        if let Some(object) = self.objects.iter_mut().find(|item| item.id == object_id) {
            object.fill = Some(color);
            return true;
        }
        false
    }

    pub fn fill_topmost_at(&mut self, point: CanvasPoint, color: SceneColor) -> Option<String> {
        for object in self.objects.iter_mut().rev() {
            let fill_hit = match &object.kind {
                ObjectKind::Connector { .. } | ObjectKind::Link { .. } => false,
                _ => object.fill_hit_test(point),
            };
            if fill_hit {
                object.fill = Some(color);
                return Some(object.id.clone());
            }
        }
        None
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    pub fn select_objects(&mut self, object_ids: Vec<String>) -> bool {
        if object_ids
            .iter()
            .any(|object_id| !self.objects.iter().any(|item| &item.id == object_id))
        {
            return false;
        }

        self.selected_object_ids = object_ids;
        true
    }

    pub fn selected_objects(&self) -> Vec<&DrawableObject> {
        self.selected_object_ids
            .iter()
            .filter_map(|selected_id| self.objects.iter().find(|item| &item.id == selected_id))
            .collect()
    }

    pub fn toggle_object_selection(&mut self, object_id: &str) -> bool {
        if !self.objects.iter().any(|item| item.id == object_id) {
            return false;
        }

        if self
            .selected_object_ids
            .iter()
            .any(|selected_id| selected_id == object_id)
        {
            self.selected_object_ids
                .retain(|selected_id| selected_id != object_id);
        } else {
            self.selected_object_ids.push(object_id.to_string());
        }
        true
    }

    pub fn select_objects_in_rect(&mut self, rect: SceneRect, additive: bool) -> Vec<String> {
        let mut object_ids = if additive {
            self.selected_object_ids.clone()
        } else {
            Vec::new()
        };
        let additions = self.object_ids_in_rect(rect);
        if additive {
            for object_id in additions {
                if !object_ids
                    .iter()
                    .any(|selected_id| selected_id == &object_id)
                {
                    object_ids.push(object_id);
                }
            }
        } else {
            object_ids = additions;
        }
        self.selected_object_ids = object_ids.clone();
        object_ids
    }

    pub fn topmost_object_at(&self, point: CanvasPoint) -> Option<&DrawableObject> {
        self.objects
            .iter()
            .rev()
            .find(|object| self.object_selection_hit_test(object, point))
    }

    pub fn topmost_fillable_object_at(&self, point: CanvasPoint) -> Option<&DrawableObject> {
        self.objects
            .iter()
            .rev()
            .find(|object| self.object_fill_hit_test(object, point))
    }

    pub fn object_ids_in_rect(&self, rect: SceneRect) -> Vec<String> {
        self.objects
            .iter()
            .filter(|object| object.visible)
            .filter(|object| self.object_bounds(object).intersects(rect))
            .map(|object| object.id.clone())
            .collect()
    }

    pub fn scene_bounds(&self) -> Option<SceneRect> {
        self.objects
            .iter()
            .map(|object| self.object_bounds(object))
            .reduce(SceneRect::union)
    }

    pub fn selected_bounds(&self) -> Option<SceneRect> {
        self.selected_objects()
            .into_iter()
            .map(|object| self.object_bounds(object))
            .reduce(SceneRect::union)
    }

    pub fn resolved_connector_path(&self, object_id: &str) -> Option<ResolvedConnectorPath> {
        let object = self.objects.iter().find(|item| item.id == object_id)?;
        match &object.kind {
            ObjectKind::Connector { connector } => self.resolve_connector_path(connector),
            ObjectKind::Link { link } => self.resolve_connector_path(&link.to_connector_spec()),
            _ => None,
        }
    }

    pub fn translate_object(&mut self, object_id: &str, dx: i32, dy: i32) -> bool {
        let Some(object) = self.objects.iter_mut().find(|item| item.id == object_id) else {
            return false;
        };
        if object.locked {
            return false;
        }
        object.translate(dx, dy);
        true
    }

    fn push(&mut self, kind: ObjectKind, stroke: SceneStroke, fill: Option<SceneColor>) -> String {
        let id = format!("obj-{}", Uuid::new_v4());
        self.objects.push(DrawableObject {
            id: id.clone(),
            kind,
            stroke,
            fill,
            visible: default_object_visible(),
            locked: default_object_locked(),
        });
        id
    }

    fn object_hit_test(&self, object: &DrawableObject, point: CanvasPoint) -> bool {
        if !object.visible {
            return false;
        }
        if let Some(connector) = connector_spec_for_object_kind(&object.kind) {
            return self
                .resolve_graph_connector_path(&connector)
                .is_some_and(|path| {
                    let tolerance = (object.stroke.width.max(1) + 4) as f64;
                    graph_route_hits_point(&path, point.into(), tolerance)
                });
        }
        object.hit_test(point)
    }

    fn object_fill_hit_test(&self, object: &DrawableObject, point: CanvasPoint) -> bool {
        if !object.visible {
            return false;
        }
        match &object.kind {
            ObjectKind::Connector { .. } | ObjectKind::Link { .. } => false,
            _ => object.fill_hit_test(point),
        }
    }

    fn object_selection_hit_test(&self, object: &DrawableObject, point: CanvasPoint) -> bool {
        if !object.visible {
            return false;
        }
        match &object.kind {
            ObjectKind::Connector { .. } | ObjectKind::Link { .. } => {
                self.object_hit_test(object, point)
            }
            _ => object.selection_hit_test(point),
        }
    }

    fn object_bounds(&self, object: &DrawableObject) -> SceneRect {
        if let Some(connector) = connector_spec_for_object_kind(&object.kind) {
            return self
                .resolve_connector_path(&connector)
                .map(|path| path.bounds())
                .unwrap_or_else(|| SceneRect::new(0, 0, 0, 0));
        }
        object.bounds()
    }

    fn resolve_graph_connector_path(&self, connector: &ConnectorSpec) -> Option<GraphRoutedPath> {
        let graph_connector = GraphConnectorSpec {
            start: connector.start.clone().into(),
            end: connector.end.clone().into(),
            kind: connector.kind.into(),
        };
        route_graph_connector(&graph_connector, |object_id| {
            self.objects
                .iter()
                .find(|item| item.id == object_id)
                .map(|object| self.object_bounds(object).into())
        })
    }

    fn resolve_connector_path(&self, connector: &ConnectorSpec) -> Option<ResolvedConnectorPath> {
        self.resolve_graph_connector_path(connector)
            .map(ResolvedConnectorPath::from)
    }
}

impl DrawableObject {
    pub fn translate(&mut self, dx: i32, dy: i32) {
        self.kind.translate(dx, dy);
    }

    pub fn hit_test(&self, point: CanvasPoint) -> bool {
        let tolerance = (self.stroke.width.max(1) + 4) as f64;

        match &self.kind {
            ObjectKind::Line { start, end } => {
                distance_to_segment(point, *start, *end) <= tolerance
            }
            ObjectKind::Connector { .. } | ObjectKind::Link { .. } => false,
            ObjectKind::Rectangle { rect } => {
                if self.fill.is_some() && rect.contains(point) {
                    return true;
                }
                point_near_rect_outline(*rect, point, tolerance)
            }
            ObjectKind::Circle { center, radius } => {
                let distance = distance(point, *center);
                if self.fill.is_some() && distance <= *radius as f64 {
                    return true;
                }
                (distance - *radius as f64).abs() <= tolerance
            }
            ObjectKind::Triangle { triangle } => {
                if self.fill.is_some() && point_in_polygon(point, triangle.as_points()) {
                    return true;
                }
                polygon_edges(triangle.as_points())
                    .iter()
                    .any(|(start, end)| distance_to_segment(point, *start, *end) <= tolerance)
            }
            ObjectKind::Polygon { points } => {
                if self.fill.is_some() && point_in_polygon(point, points) {
                    return true;
                }
                polygon_edges(points)
                    .iter()
                    .any(|(start, end)| distance_to_segment(point, *start, *end) <= tolerance)
            }
            ObjectKind::Stroke { points } => points
                .windows(2)
                .any(|segment| distance_to_segment(point, segment[0], segment[1]) <= tolerance),
            ObjectKind::Text { text } => approximate_text_bounds(text).contains(point),
        }
    }

    pub fn fill_hit_test(&self, point: CanvasPoint) -> bool {
        match &self.kind {
            ObjectKind::Rectangle { rect } => rect.contains(point),
            ObjectKind::Circle { center, radius } => distance(point, *center) <= *radius as f64,
            ObjectKind::Triangle { triangle } => point_in_polygon(point, triangle.as_points()),
            ObjectKind::Polygon { points } => point_in_polygon(point, points),
            ObjectKind::Text { text } => approximate_text_bounds(text).contains(point),
            ObjectKind::Line { .. }
            | ObjectKind::Stroke { .. }
            | ObjectKind::Connector { .. }
            | ObjectKind::Link { .. } => false,
        }
    }

    pub fn selection_hit_test(&self, point: CanvasPoint) -> bool {
        match &self.kind {
            ObjectKind::Rectangle { .. }
            | ObjectKind::Circle { .. }
            | ObjectKind::Triangle { .. }
            | ObjectKind::Polygon { .. }
            | ObjectKind::Text { .. } => self.fill_hit_test(point),
            ObjectKind::Line { .. } | ObjectKind::Stroke { .. } => self.hit_test(point),
            ObjectKind::Connector { .. } | ObjectKind::Link { .. } => false,
        }
    }

    pub fn bounds(&self) -> SceneRect {
        match &self.kind {
            ObjectKind::Line { start, end } => SceneRect::from_points(*start, *end),
            ObjectKind::Connector { .. } | ObjectKind::Link { .. } => SceneRect::new(0, 0, 0, 0),
            ObjectKind::Rectangle { rect } => *rect,
            ObjectKind::Circle { center, radius } => {
                SceneRect::new(center.x - radius, center.y - radius, radius * 2, radius * 2)
            }
            ObjectKind::Triangle { triangle } => points_bounds(triangle.as_points()),
            ObjectKind::Polygon { points } => points_bounds(points),
            ObjectKind::Stroke { points } => points_bounds(points),
            ObjectKind::Text { text } => approximate_text_bounds(text),
        }
    }
}

impl ObjectKind {
    fn translate(&mut self, dx: i32, dy: i32) {
        match self {
            ObjectKind::Line { start, end } => {
                *start = start.translate(dx, dy);
                *end = end.translate(dx, dy);
            }
            ObjectKind::Connector { connector } => {
                connector.start.translate(dx, dy);
                connector.end.translate(dx, dy);
            }
            ObjectKind::Link { link } => {
                link.source.endpoint.translate(dx, dy);
                link.target.endpoint.translate(dx, dy);
            }
            ObjectKind::Rectangle { rect } => {
                rect.x += dx;
                rect.y += dy;
            }
            ObjectKind::Circle { center, .. } => {
                *center = center.translate(dx, dy);
            }
            ObjectKind::Triangle { triangle } => {
                for point in &mut triangle.points {
                    *point = point.translate(dx, dy);
                }
            }
            ObjectKind::Polygon { points } | ObjectKind::Stroke { points } => {
                for point in points {
                    *point = point.translate(dx, dy);
                }
            }
            ObjectKind::Text { text } => {
                text.position = text.position.translate(dx, dy);
            }
        }
    }
}

impl ConnectorEndpoint {
    fn translate(&mut self, dx: i32, dy: i32) {
        match self {
            ConnectorEndpoint::Free { point } => {
                *point = point.translate(dx, dy);
            }
            ConnectorEndpoint::Bound { offset, .. } => {
                *offset = offset.translate(dx, dy);
            }
        }
    }
}

impl ResolvedConnectorPath {
    fn bounds(&self) -> SceneRect {
        match self {
            ResolvedConnectorPath::Polyline(points) => points_bounds(points),
            ResolvedConnectorPath::CubicBezier(segments) => {
                let points = segments
                    .iter()
                    .flat_map(|segment| {
                        [
                            segment.start,
                            segment.control_start,
                            segment.control_end,
                            segment.end,
                        ]
                    })
                    .collect::<Vec<_>>();
                points_bounds(&points)
            }
        }
    }
}

impl From<ConnectorAnchor> for GraphAnchorKind {
    fn from(value: ConnectorAnchor) -> Self {
        match value {
            ConnectorAnchor::Center => GraphAnchorKind::Center,
            ConnectorAnchor::North => GraphAnchorKind::North,
            ConnectorAnchor::South => GraphAnchorKind::South,
            ConnectorAnchor::East => GraphAnchorKind::East,
            ConnectorAnchor::West => GraphAnchorKind::West,
            ConnectorAnchor::NorthEast => GraphAnchorKind::NorthEast,
            ConnectorAnchor::NorthWest => GraphAnchorKind::NorthWest,
            ConnectorAnchor::SouthEast => GraphAnchorKind::SouthEast,
            ConnectorAnchor::SouthWest => GraphAnchorKind::SouthWest,
        }
    }
}

impl From<ConnectorEndpoint> for GraphEndpoint {
    fn from(value: ConnectorEndpoint) -> Self {
        match value {
            ConnectorEndpoint::Free { point } => GraphEndpoint::Free(point.into()),
            ConnectorEndpoint::Bound {
                object_id,
                anchor,
                offset,
            } => GraphEndpoint::Bound {
                node_id: object_id,
                anchor: anchor.into(),
                offset: offset.into(),
            },
        }
    }
}

impl From<SceneConnectorKind> for GraphConnectorKind {
    fn from(value: SceneConnectorKind) -> Self {
        match value {
            SceneConnectorKind::Straight => GraphConnectorKind::Straight,
            SceneConnectorKind::Orthogonal => GraphConnectorKind::Orthogonal,
            SceneConnectorKind::Spline => GraphConnectorKind::Spline,
        }
    }
}

impl From<LinkRoutingPreference> for SceneConnectorKind {
    fn from(value: LinkRoutingPreference) -> Self {
        match value {
            LinkRoutingPreference::Straight => SceneConnectorKind::Straight,
            LinkRoutingPreference::Orthogonal => SceneConnectorKind::Orthogonal,
            LinkRoutingPreference::Spline => SceneConnectorKind::Spline,
        }
    }
}

fn connector_spec_for_object_kind(kind: &ObjectKind) -> Option<ConnectorSpec> {
    match kind {
        ObjectKind::Connector { connector } => Some(connector.clone()),
        ObjectKind::Link { link } => Some(link.to_connector_spec()),
        _ => None,
    }
}

impl From<GraphPoint> for CanvasPoint {
    fn from(value: GraphPoint) -> Self {
        Self::new(value.x.round() as i32, value.y.round() as i32)
    }
}

impl From<CanvasPoint> for GraphPoint {
    fn from(value: CanvasPoint) -> Self {
        GraphPoint::new(value.x as f64, value.y as f64)
    }
}

impl From<SceneRect> for GraphRect {
    fn from(value: SceneRect) -> Self {
        GraphRect::new(
            value.x as f64,
            value.y as f64,
            value.width as f64,
            value.height as f64,
        )
    }
}

impl From<GraphRoutedPath> for ResolvedConnectorPath {
    fn from(value: GraphRoutedPath) -> Self {
        match value {
            GraphRoutedPath::Polyline(points) => {
                ResolvedConnectorPath::Polyline(points.into_iter().map(Into::into).collect())
            }
            GraphRoutedPath::CubicBezier(segments) => {
                ResolvedConnectorPath::CubicBezier(segments.into_iter().map(Into::into).collect())
            }
        }
    }
}

impl From<GraphCubicBezierSegment> for ResolvedCubicBezierSegment {
    fn from(value: GraphCubicBezierSegment) -> Self {
        Self {
            start: value.start.into(),
            control_start: value.control_start.into(),
            control_end: value.control_end.into(),
            end: value.end.into(),
        }
    }
}

fn approximate_text_bounds(text: &TextObject) -> SceneRect {
    let width = (text.content.chars().count() as i32 * 8).max(24);
    SceneRect::new(text.position.x, text.position.y, width, 18)
}

fn point_near_rect_outline(rect: SceneRect, point: CanvasPoint, tolerance: f64) -> bool {
    let top_left = CanvasPoint::new(rect.x, rect.y);
    let top_right = CanvasPoint::new(rect.x + rect.width, rect.y);
    let bottom_left = CanvasPoint::new(rect.x, rect.y + rect.height);
    let bottom_right = CanvasPoint::new(rect.x + rect.width, rect.y + rect.height);

    distance_to_segment(point, top_left, top_right) <= tolerance
        || distance_to_segment(point, top_right, bottom_right) <= tolerance
        || distance_to_segment(point, bottom_right, bottom_left) <= tolerance
        || distance_to_segment(point, bottom_left, top_left) <= tolerance
}

fn distance(a: CanvasPoint, b: CanvasPoint) -> f64 {
    let dx = (a.x - b.x) as f64;
    let dy = (a.y - b.y) as f64;
    (dx * dx + dy * dy).sqrt()
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

fn distance_to_segment(point: CanvasPoint, start: CanvasPoint, end: CanvasPoint) -> f64 {
    let px = point.x as f64;
    let py = point.y as f64;
    let x1 = start.x as f64;
    let y1 = start.y as f64;
    let x2 = end.x as f64;
    let y2 = end.y as f64;
    let dx = x2 - x1;
    let dy = y2 - y1;

    if dx == 0.0 && dy == 0.0 {
        return ((px - x1).powi(2) + (py - y1).powi(2)).sqrt();
    }

    let t = (((px - x1) * dx) + ((py - y1) * dy)) / (dx * dx + dy * dy);
    let t = t.clamp(0.0, 1.0);
    let proj_x = x1 + t * dx;
    let proj_y = y1 + t * dy;
    ((px - proj_x).powi(2) + (py - proj_y).powi(2)).sqrt()
}

fn points_bounds(points: &[CanvasPoint]) -> SceneRect {
    let mut left = i32::MAX;
    let mut top = i32::MAX;
    let mut right = i32::MIN;
    let mut bottom = i32::MIN;

    for point in points {
        left = left.min(point.x);
        top = top.min(point.y);
        right = right.max(point.x);
        bottom = bottom.max(point.y);
    }

    if points.is_empty() {
        return SceneRect::new(0, 0, 0, 0);
    }

    SceneRect::new(left, top, right - left, bottom - top)
}

const fn default_scene_version() -> u32 {
    1
}

const fn default_scene_background() -> SceneColor {
    SceneColor::rgb(250, 250, 249)
}

const fn default_stroke_width() -> i32 {
    2
}

const fn opaque_alpha() -> u8 {
    255
}

const fn default_object_visible() -> bool {
    true
}

const fn default_object_locked() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_fill_hits_inside_shape() {
        let mut scene = SceneDocument::default();
        let stroke = SceneStroke::default();
        let object_id = scene.add_rectangle(
            SceneRect::new(10, 10, 100, 80),
            stroke,
            Some(SceneColor::rgb(255, 0, 0)),
        );

        let object = scene
            .objects
            .iter()
            .find(|item| item.id == object_id)
            .unwrap();
        assert!(object.hit_test(CanvasPoint::new(50, 40)));
    }

    #[test]
    fn fill_topmost_fills_inside_unfilled_rectangle() {
        let mut scene = SceneDocument::default();
        let object_id = scene.add_rectangle(
            SceneRect::new(10, 10, 100, 80),
            SceneStroke::default(),
            None,
        );

        let filled = scene.fill_topmost_at(CanvasPoint::new(50, 40), SceneColor::rgb(0, 255, 0));
        assert_eq!(filled.as_deref(), Some(object_id.as_str()));

        let object = scene
            .objects
            .iter()
            .find(|item| item.id == object_id)
            .unwrap();
        assert_eq!(object.fill, Some(SceneColor::rgb(0, 255, 0)));
    }

    #[test]
    fn fill_topmost_fills_inside_unfilled_circle() {
        let mut scene = SceneDocument::default();
        let object_id =
            scene.add_circle(CanvasPoint::new(60, 60), 30, SceneStroke::default(), None);

        let filled = scene.fill_topmost_at(CanvasPoint::new(60, 60), SceneColor::rgb(0, 0, 255));
        assert_eq!(filled.as_deref(), Some(object_id.as_str()));

        let object = scene
            .objects
            .iter()
            .find(|item| item.id == object_id)
            .unwrap();
        assert_eq!(object.fill, Some(SceneColor::rgb(0, 0, 255)));
    }

    #[test]
    fn fill_topmost_fills_inside_unfilled_triangle() {
        let mut scene = SceneDocument::default();
        let object_id = scene.add_triangle(
            TriangleObject {
                points: [
                    CanvasPoint::new(60, 10),
                    CanvasPoint::new(10, 100),
                    CanvasPoint::new(110, 100),
                ],
            },
            SceneStroke::default(),
            None,
        );

        let filled = scene.fill_topmost_at(CanvasPoint::new(60, 60), SceneColor::rgb(255, 128, 0));
        assert_eq!(filled.as_deref(), Some(object_id.as_str()));

        let object = scene
            .objects
            .iter()
            .find(|item| item.id == object_id)
            .unwrap();
        assert_eq!(object.fill, Some(SceneColor::rgb(255, 128, 0)));
    }

    #[test]
    fn fill_topmost_fills_inside_unfilled_polygon() {
        let mut scene = SceneDocument::default();
        let object_id = scene
            .add_polygon(
                vec![
                    CanvasPoint::new(20, 20),
                    CanvasPoint::new(120, 20),
                    CanvasPoint::new(100, 90),
                    CanvasPoint::new(40, 120),
                ],
                SceneStroke::default(),
                None,
            )
            .unwrap();

        let filled = scene.fill_topmost_at(CanvasPoint::new(70, 60), SceneColor::rgb(0, 180, 255));
        assert_eq!(filled.as_deref(), Some(object_id.as_str()));

        let object = scene
            .objects
            .iter()
            .find(|item| item.id == object_id)
            .unwrap();
        assert_eq!(object.fill, Some(SceneColor::rgb(0, 180, 255)));
    }

    #[test]
    fn fill_topmost_prefers_latest_object() {
        let mut scene = SceneDocument::default();
        scene.add_rectangle(
            SceneRect::new(0, 0, 100, 100),
            SceneStroke::default(),
            Some(SceneColor::rgb(10, 10, 10)),
        );
        let front_id = scene.add_circle(
            CanvasPoint::new(50, 50),
            30,
            SceneStroke::default(),
            Some(SceneColor::rgb(20, 20, 20)),
        );

        let filled = scene.fill_topmost_at(CanvasPoint::new(50, 50), SceneColor::rgb(0, 255, 0));
        assert_eq!(filled.as_deref(), Some(front_id.as_str()));
    }

    #[test]
    fn delete_object_removes_item() {
        let mut scene = SceneDocument::default();
        let object_id = scene.add_line(
            CanvasPoint::new(0, 0),
            CanvasPoint::new(10, 10),
            SceneStroke::default(),
        );

        assert!(scene.delete_object(&object_id));
        assert!(scene.objects.is_empty());
    }

    #[test]
    fn select_object_tracks_selection() {
        let mut scene = SceneDocument::default();
        let object_id = scene.add_line(
            CanvasPoint::new(0, 0),
            CanvasPoint::new(10, 10),
            SceneStroke::default(),
        );

        assert!(scene.select_objects(vec![object_id.clone()]));
        assert_eq!(scene.selected_object_ids, vec![object_id.clone()]);
        assert_eq!(
            scene
                .selected_objects()
                .into_iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec![object_id.as_str()]
        );
    }

    #[test]
    fn delete_object_clears_selection() {
        let mut scene = SceneDocument::default();
        let object_id = scene.add_line(
            CanvasPoint::new(0, 0),
            CanvasPoint::new(10, 10),
            SceneStroke::default(),
        );
        assert!(scene.select_objects(vec![object_id.clone()]));

        assert!(scene.delete_object(&object_id));
        assert!(scene.selected_object_ids.is_empty());
    }

    #[test]
    fn delete_objects_removes_multiple_items_and_selection() {
        let mut scene = SceneDocument::default();
        let first_id = scene.add_line(
            CanvasPoint::new(0, 0),
            CanvasPoint::new(10, 10),
            SceneStroke::default(),
        );
        let second_id =
            scene.add_rectangle(SceneRect::new(10, 10, 20, 20), SceneStroke::default(), None);
        let kept_id = scene.add_circle(CanvasPoint::new(50, 50), 10, SceneStroke::default(), None);
        assert!(scene.select_objects(vec![first_id.clone(), second_id.clone()]));

        let deleted = scene.delete_objects(&[first_id.clone(), second_id.clone()]);
        assert_eq!(deleted, 2);
        assert!(scene.selected_object_ids.is_empty());
        assert_eq!(
            scene
                .objects
                .iter()
                .map(|object| object.id.as_str())
                .collect::<Vec<_>>(),
            vec![kept_id.as_str()]
        );
    }

    #[test]
    fn topmost_object_at_selects_inside_unfilled_rectangle() {
        let mut scene = SceneDocument::default();
        let object_id = scene.add_rectangle(
            SceneRect::new(10, 10, 100, 80),
            SceneStroke::default(),
            None,
        );

        let selected = scene.topmost_object_at(CanvasPoint::new(50, 40)).unwrap();
        assert_eq!(selected.id, object_id);
    }

    #[test]
    fn topmost_object_at_selects_inside_unfilled_circle() {
        let mut scene = SceneDocument::default();
        let object_id =
            scene.add_circle(CanvasPoint::new(60, 60), 30, SceneStroke::default(), None);

        let selected = scene.topmost_object_at(CanvasPoint::new(60, 60)).unwrap();
        assert_eq!(selected.id, object_id);
    }

    #[test]
    fn topmost_object_at_selects_inside_unfilled_triangle() {
        let mut scene = SceneDocument::default();
        let object_id = scene.add_triangle(
            TriangleObject {
                points: [
                    CanvasPoint::new(60, 10),
                    CanvasPoint::new(10, 100),
                    CanvasPoint::new(110, 100),
                ],
            },
            SceneStroke::default(),
            None,
        );

        let selected = scene.topmost_object_at(CanvasPoint::new(60, 60)).unwrap();
        assert_eq!(selected.id, object_id);
    }

    #[test]
    fn topmost_object_at_selects_inside_unfilled_polygon() {
        let mut scene = SceneDocument::default();
        let object_id = scene
            .add_polygon(
                vec![
                    CanvasPoint::new(20, 20),
                    CanvasPoint::new(120, 20),
                    CanvasPoint::new(100, 90),
                    CanvasPoint::new(40, 120),
                ],
                SceneStroke::default(),
                None,
            )
            .unwrap();

        let selected = scene.topmost_object_at(CanvasPoint::new(70, 60)).unwrap();
        assert_eq!(selected.id, object_id);
    }

    #[test]
    fn canvas_point_deserializes_from_string_and_array() {
        let point: CanvasPoint = serde_json::from_str(r#""380,392""#).unwrap();
        assert_eq!(point, CanvasPoint::new(380, 392));

        let point: CanvasPoint = serde_json::from_str("[640,55]").unwrap();
        assert_eq!(point, CanvasPoint::new(640, 55));
    }

    #[test]
    fn scene_rect_deserializes_from_string_and_array() {
        let rect: SceneRect = serde_json::from_str(r#""650,120,150,24""#).unwrap();
        assert_eq!(rect, SceneRect::new(650, 120, 150, 24));

        let rect: SceneRect = serde_json::from_str("[776,144,150,24]").unwrap();
        assert_eq!(rect, SceneRect::new(776, 144, 150, 24));
    }

    #[test]
    fn object_ids_in_rect_returns_intersecting_objects() {
        let mut scene = SceneDocument::default();
        let first = scene.add_rectangle(
            SceneRect::new(10, 10, 100, 80),
            SceneStroke::default(),
            None,
        );
        let second = scene.add_circle(CanvasPoint::new(260, 260), 40, SceneStroke::default(), None);

        let ids = scene.object_ids_in_rect(SceneRect::new(0, 0, 150, 120));
        assert_eq!(ids, vec![first]);
        assert!(!ids.iter().any(|object_id| object_id == &second));
    }

    #[test]
    fn scene_bounds_cover_all_objects() {
        let mut scene = SceneDocument::default();
        scene.add_rectangle(SceneRect::new(10, 20, 40, 30), SceneStroke::default(), None);
        scene.add_circle(CanvasPoint::new(120, 80), 15, SceneStroke::default(), None);

        assert_eq!(scene.scene_bounds(), Some(SceneRect::new(10, 20, 125, 75)));
    }

    #[test]
    fn selected_bounds_cover_only_selected_objects() {
        let mut scene = SceneDocument::default();
        let first =
            scene.add_rectangle(SceneRect::new(10, 20, 40, 30), SceneStroke::default(), None);
        scene.add_circle(CanvasPoint::new(120, 80), 15, SceneStroke::default(), None);
        assert!(scene.select_objects(vec![first]));

        assert_eq!(
            scene.selected_bounds(),
            Some(SceneRect::new(10, 20, 40, 30))
        );
    }

    #[test]
    fn bound_connectors_follow_moved_objects() {
        let mut scene = SceneDocument::default();
        let source_id =
            scene.add_rectangle(SceneRect::new(10, 20, 40, 30), SceneStroke::default(), None);
        let target_id = scene.add_rectangle(
            SceneRect::new(160, 40, 60, 50),
            SceneStroke::default(),
            None,
        );
        let connector_id = scene.add_connector(
            ConnectorSpec {
                start: ConnectorEndpoint::Bound {
                    object_id: source_id.clone(),
                    anchor: ConnectorAnchor::East,
                    offset: CanvasPoint::new(0, 0),
                },
                end: ConnectorEndpoint::Bound {
                    object_id: target_id.clone(),
                    anchor: ConnectorAnchor::West,
                    offset: CanvasPoint::new(0, 0),
                },
                kind: SceneConnectorKind::Orthogonal,
            },
            SceneStroke::default(),
        );

        assert_eq!(
            scene.resolved_connector_path(&connector_id),
            Some(ResolvedConnectorPath::Polyline(vec![
                CanvasPoint::new(50, 35),
                CanvasPoint::new(105, 35),
                CanvasPoint::new(105, 65),
                CanvasPoint::new(160, 65),
            ]))
        );

        assert!(scene.translate_object(&source_id, 30, 20));

        assert_eq!(
            scene.resolved_connector_path(&connector_id),
            Some(ResolvedConnectorPath::Polyline(vec![
                CanvasPoint::new(80, 55),
                CanvasPoint::new(120, 55),
                CanvasPoint::new(120, 65),
                CanvasPoint::new(160, 65),
            ]))
        );
    }

    #[test]
    fn semantic_link_converts_to_connector_spec() {
        let link = LinkObject {
            source: LinkEnd {
                endpoint: ConnectorEndpoint::Bound {
                    object_id: "source".to_string(),
                    anchor: ConnectorAnchor::East,
                    offset: CanvasPoint::new(4, 0),
                },
                label: Some("source role".to_string()),
            },
            target: LinkEnd {
                endpoint: ConnectorEndpoint::Bound {
                    object_id: "target".to_string(),
                    anchor: ConnectorAnchor::West,
                    offset: CanvasPoint::new(-4, 0),
                },
                label: Some("target role".to_string()),
            },
            routing: LinkRoutingPreference::Orthogonal,
            label: Some("owns".to_string()),
        };

        assert_eq!(
            link.to_connector_spec(),
            ConnectorSpec {
                start: ConnectorEndpoint::Bound {
                    object_id: "source".to_string(),
                    anchor: ConnectorAnchor::East,
                    offset: CanvasPoint::new(4, 0),
                },
                end: ConnectorEndpoint::Bound {
                    object_id: "target".to_string(),
                    anchor: ConnectorAnchor::West,
                    offset: CanvasPoint::new(-4, 0),
                },
                kind: SceneConnectorKind::Orthogonal,
            }
        );
    }

    #[test]
    fn semantic_links_resolve_through_connector_paths() {
        let mut scene = SceneDocument::default();
        let source_id =
            scene.add_rectangle(SceneRect::new(10, 20, 40, 30), SceneStroke::default(), None);
        let target_id = scene.add_rectangle(
            SceneRect::new(160, 40, 60, 50),
            SceneStroke::default(),
            None,
        );
        let link_id = scene.add_link(
            LinkObject {
                source: LinkEnd {
                    endpoint: ConnectorEndpoint::Bound {
                        object_id: source_id.clone(),
                        anchor: ConnectorAnchor::East,
                        offset: CanvasPoint::new(0, 0),
                    },
                    label: Some("source".to_string()),
                },
                target: LinkEnd {
                    endpoint: ConnectorEndpoint::Bound {
                        object_id: target_id.clone(),
                        anchor: ConnectorAnchor::West,
                        offset: CanvasPoint::new(0, 0),
                    },
                    label: Some("target".to_string()),
                },
                routing: LinkRoutingPreference::Orthogonal,
                label: Some("references".to_string()),
            },
            SceneStroke::default(),
        );

        let object = scene
            .objects
            .iter()
            .find(|item| item.id == link_id)
            .unwrap();
        match &object.kind {
            ObjectKind::Link { link } => {
                assert_eq!(link.label.as_deref(), Some("references"));
                assert_eq!(link.routing, LinkRoutingPreference::Orthogonal);
            }
            other => panic!("expected link object, got {other:?}"),
        }

        assert_eq!(
            scene.resolved_connector_path(&link_id),
            Some(ResolvedConnectorPath::Polyline(vec![
                CanvasPoint::new(50, 35),
                CanvasPoint::new(105, 35),
                CanvasPoint::new(105, 65),
                CanvasPoint::new(160, 65),
            ]))
        );
    }

    #[test]
    fn connector_hit_testing_uses_resolved_path_geometry() {
        let mut scene = SceneDocument::default();
        let source_id =
            scene.add_rectangle(SceneRect::new(10, 20, 40, 30), SceneStroke::default(), None);
        let target_id = scene.add_rectangle(
            SceneRect::new(160, 40, 60, 50),
            SceneStroke::default(),
            None,
        );
        let connector_id = scene.add_connector(
            ConnectorSpec {
                start: ConnectorEndpoint::Bound {
                    object_id: source_id,
                    anchor: ConnectorAnchor::East,
                    offset: CanvasPoint::new(0, 0),
                },
                end: ConnectorEndpoint::Bound {
                    object_id: target_id,
                    anchor: ConnectorAnchor::West,
                    offset: CanvasPoint::new(0, 0),
                },
                kind: SceneConnectorKind::Orthogonal,
            },
            SceneStroke::default(),
        );

        let hit = scene
            .topmost_object_at(CanvasPoint::new(105, 63))
            .expect("connector hit");

        assert_eq!(hit.id, connector_id);
    }

    #[test]
    fn toggle_object_selection_adds_and_removes_existing_objects() {
        let mut scene = SceneDocument::default();
        let first_id =
            scene.add_rectangle(SceneRect::new(10, 10, 40, 30), SceneStroke::default(), None);
        let second_id =
            scene.add_circle(CanvasPoint::new(120, 60), 15, SceneStroke::default(), None);

        assert!(scene.toggle_object_selection(&first_id));
        assert_eq!(scene.selected_object_ids, vec![first_id.clone()]);

        assert!(scene.toggle_object_selection(&second_id));
        assert_eq!(
            scene.selected_object_ids,
            vec![first_id.clone(), second_id.clone()]
        );

        assert!(scene.toggle_object_selection(&first_id));
        assert_eq!(scene.selected_object_ids, vec![second_id]);

        assert!(!scene.toggle_object_selection("missing"));
    }

    #[test]
    fn select_objects_in_rect_supports_replace_and_additive_modes() {
        let mut scene = SceneDocument::default();
        let first_id =
            scene.add_rectangle(SceneRect::new(10, 10, 40, 30), SceneStroke::default(), None);
        let second_id =
            scene.add_rectangle(SceneRect::new(80, 20, 40, 30), SceneStroke::default(), None);
        let third_id = scene.add_rectangle(
            SceneRect::new(200, 30, 40, 30),
            SceneStroke::default(),
            None,
        );

        let replaced = scene.select_objects_in_rect(SceneRect::new(0, 0, 140, 80), false);
        assert_eq!(replaced, vec![first_id.clone(), second_id.clone()]);
        assert_eq!(scene.selected_object_ids, replaced);

        let additive = scene.select_objects_in_rect(SceneRect::new(180, 0, 100, 100), true);
        assert_eq!(additive, vec![first_id, second_id, third_id]);
        assert_eq!(scene.selected_object_ids, additive);
    }

    #[test]
    fn duplicate_object_creates_shifted_copy_and_selectable_id() {
        let mut scene = SceneDocument::default();
        let original_id = scene.add_rectangle(
            SceneRect::new(10, 20, 40, 30),
            SceneStroke::default(),
            Some(SceneColor::rgb(10, 20, 30)),
        );

        let duplicate_id = scene
            .duplicate_object(&original_id, 24, 16)
            .expect("duplicate object");

        assert_ne!(duplicate_id, original_id);
        assert_eq!(scene.objects.len(), 2);

        let duplicate = scene
            .objects
            .iter()
            .find(|object| object.id == duplicate_id)
            .expect("duplicate object stored");
        assert_eq!(duplicate.fill, Some(SceneColor::rgb(10, 20, 30)));
        assert_eq!(duplicate.bounds(), SceneRect::new(34, 36, 40, 30));
    }

    #[test]
    fn hidden_objects_are_ignored_by_hit_testing_and_rect_selection() {
        let mut scene = SceneDocument::default();
        let object_id =
            scene.add_rectangle(SceneRect::new(10, 10, 80, 50), SceneStroke::default(), None);
        assert!(scene.set_object_visibility(&object_id, false));

        assert!(scene.topmost_object_at(CanvasPoint::new(40, 30)).is_none());
        assert!(
            scene
                .select_objects_in_rect(SceneRect::new(0, 0, 120, 100), false)
                .is_empty()
        );
    }

    #[test]
    fn locked_objects_reject_translation_until_unlocked() {
        let mut scene = SceneDocument::default();
        let object_id =
            scene.add_rectangle(SceneRect::new(10, 10, 40, 30), SceneStroke::default(), None);

        assert!(scene.set_object_locked(&object_id, true));
        assert!(!scene.translate_object(&object_id, 20, 10));
        assert_eq!(
            scene
                .objects
                .iter()
                .find(|object| object.id == object_id)
                .expect("object")
                .bounds(),
            SceneRect::new(10, 10, 40, 30)
        );

        assert!(scene.set_object_locked(&object_id, false));
        assert!(scene.translate_object(&object_id, 20, 10));
        assert_eq!(
            scene
                .objects
                .iter()
                .find(|object| object.id == object_id)
                .expect("object")
                .bounds(),
            SceneRect::new(30, 20, 40, 30)
        );
    }

    #[test]
    fn z_order_operations_change_topmost_selection() {
        let mut scene = SceneDocument::default();
        let back_id = scene.add_rectangle(
            SceneRect::new(10, 10, 100, 80),
            SceneStroke::default(),
            None,
        );
        let front_id = scene.add_circle(CanvasPoint::new(60, 50), 35, SceneStroke::default(), None);

        assert_eq!(
            scene
                .topmost_object_at(CanvasPoint::new(60, 50))
                .expect("front object")
                .id,
            front_id
        );

        assert!(scene.move_object_to_back(&front_id));
        assert_eq!(
            scene
                .topmost_object_at(CanvasPoint::new(60, 50))
                .expect("back object now topmost")
                .id,
            back_id
        );

        assert!(scene.move_object_to_front(&front_id));
        assert_eq!(
            scene
                .topmost_object_at(CanvasPoint::new(60, 50))
                .expect("front restored")
                .id,
            front_id
        );
    }
}
