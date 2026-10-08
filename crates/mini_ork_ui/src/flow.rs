//! The run flow map, part A: one horizontal track from **Ticket in** through
//! **Design** and the **Build** fan-out to the **Integrate** pill.
//!
//! The data is the run page's top-level `flow` object (`mini_ork/ide_pages/
//! run_flow.py::build_flow`): the ticket, the design steps with their revise
//! loops, the build packages with an Implementer / critic lane each, and the
//! integrate node with its check tiers. Part B adds Review / Merge / Rollback
//! and retires the older `dag` section below the map.
//!
//! Connectors are one `gpui::canvas` painting stroked paths — the same idiom
//! `graph.rs` and `thumb.rs` use — while every marker, label, package box and
//! pill is an absolutely positioned div over it. That keeps the labels
//! selectable and lets a marker carry the click that opens its node.
//!
//! Geometry is decided once in [`Layout::compute`] so the canvas (which needs
//! the endpoints) and the divs (which need the same anchors) cannot drift.

use std::collections::HashMap;

use gpui::{
    AnyElement, Bounds, Context, Div, FontWeight, Hsla, PathBuilder, Pixels, Point, ScrollHandle,
    SharedString, Window, canvas, point, px,
};
use serde_json::Value;
use ui::{ScrollAxes, Scrollbars, WithScrollbar as _, prelude::*};

use crate::{
    page::{MiniOrkPage, arr, str_of},
    palette::{self as p, c, ca},
};

/// The y of the track's centre line, in the map's own coordinates.
const TRACK_Y: f32 = 108.;
/// Edges are 3px; an edge into a pending (or skipped) step is 2px.
const EDGE_W: f32 = 3.;
const EDGE_W_PENDING: f32 = 2.;
/// A design marker: the 22px ring and the 9px dot inside it.
const MARKER_D: f32 = 22.;
const DOT_D: f32 = 9.;
/// Horizontal pitch between two design markers.
const STEP_PITCH: f32 = 132.;
/// The ticket's bar, and the block of text beside it.
const TICKET_W: f32 = 6.;
const TICKET_H: f32 = 56.;
const TICKET_LABEL_W: f32 = 150.;
/// A build package box: a fixed head (the role-label row) plus one 30px row
/// per lane, and the width its Implementer / critic marker pair needs.
const PKG_W: f32 = 236.;
const PKG_HEAD: f32 = 26.;
const LANE_H: f32 = 30.;
const PKG_GAP: f32 = 14.;
/// The integrate pill, and the width its tier rows need below it.
const PILL_W: f32 = 100.;
const PILL_H: f32 = 40.;
const TIERS_W: f32 = 176.;
/// A revise arc leaves the track this far above it, peaks at
/// `TRACK_Y - ARC_APEX_DY`, and stacks its "revise" label and its
/// "approved in round N" pill above the peak.
const ARC_RISE: f32 = 14.;
const ARC_CTRL_RISE: f32 = 58.;
const ARC_APEX_DY: f32 = 47.;

/// The map, parsed out of the page's `flow` object. Every part is optional:
/// the builder is fail-soft, so a run with no build (or no verifiers) simply
/// has fewer parts to draw.
struct Flow {
    ticket_id: String,
    design: Vec<Step>,
    design_loops: Vec<ReviseLoop>,
    packages: Vec<Package>,
    build_loops: Vec<ReviseLoop>,
    integrate: Option<Step>,
    tiers: Vec<Tier>,
}

/// One node of the map, as the flow model spells it.
struct Step {
    id: String,
    label: String,
    sub: String,
    state: String,
    score: String,
    action: Value,
}

struct ReviseLoop {
    from: String,
    to: String,
    approved_round: Option<u64>,
}

struct Package {
    label: String,
    lanes: Vec<Lane>,
}

struct Lane {
    role: String,
    node: Step,
    critic: Option<Step>,
}

struct Tier {
    label: String,
    state: String,
    passed: f32,
    total: f32,
}

impl Flow {
    fn parse(flow: &Value) -> Self {
        let ticket = flow.get("ticket").cloned().unwrap_or(Value::Null);
        let design = flow.get("design").cloned().unwrap_or(Value::Null);
        let build = flow.get("build").cloned().unwrap_or(Value::Null);
        let integrate = flow.get("integrate").cloned().unwrap_or(Value::Null);
        Self {
            ticket_id: str_of(&ticket, "id"),
            design: arr(&design, "nodes").iter().filter_map(parse_step).collect(),
            design_loops: parse_loops(&design),
            packages: arr(&build, "packages").iter().filter_map(parse_package).collect(),
            build_loops: parse_loops(&build),
            integrate: integrate.get("node").and_then(parse_step),
            tiers: arr(&integrate, "tiers").iter().filter_map(parse_tier).collect(),
        }
    }
}

fn parse_step(value: &Value) -> Option<Step> {
    if !value.is_object() {
        return None;
    }
    Some(Step {
        id: str_of(value, "id"),
        label: str_of(value, "label"),
        sub: str_of(value, "sub"),
        state: str_of(value, "state"),
        score: str_of(value, "score"),
        action: value.get("do").cloned().unwrap_or(Value::Null),
    })
}

fn parse_package(value: &Value) -> Option<Package> {
    if !value.is_object() {
        return None;
    }
    let lanes = arr(value, "lanes")
        .iter()
        .filter_map(|lane| {
            if !lane.is_object() {
                return None;
            }
            Some(Lane {
                role: str_of(lane, "role"),
                node: parse_step(lane.get("node")?)?,
                critic: lane.get("critic").and_then(parse_step),
            })
        })
        .collect();
    Some(Package { label: str_of(value, "label"), lanes })
}

fn parse_loops(value: &Value) -> Vec<ReviseLoop> {
    arr(value, "revise")
        .iter()
        .filter_map(|looped| {
            if !looped.is_object() {
                return None;
            }
            Some(ReviseLoop {
                from: str_of(looped, "from"),
                to: str_of(looped, "to"),
                approved_round: looped.get("approved_round").and_then(Value::as_u64),
            })
        })
        .collect()
}

fn parse_tier(value: &Value) -> Option<Tier> {
    if !value.is_object() {
        return None;
    }
    Some(Tier {
        label: str_of(value, "label"),
        state: str_of(value, "state"),
        passed: value.get("passed").and_then(Value::as_f64).unwrap_or(0.) as f32,
        total: value.get("total").and_then(Value::as_f64).unwrap_or(0.) as f32,
    })
}

/// Where every marker, box and pill goes. Only x/y arithmetic here — no
/// elements — so the canvas and the divs read the same numbers.
struct Layout {
    width: f32,
    height: f32,
    ticket_x: f32,
    ticket_label_x: f32,
    /// The design markers' centre x, in draw order.
    design_x: Vec<f32>,
    /// Package boxes, in draw order, with their lanes' marker x.
    packages: Vec<PackageSpot>,
    fan_x: f32,
    fan_in_x: f32,
    integrate_x: f32,
    /// Revise arcs, as the two marker x they span.
    arcs: Vec<Arc>,
}

struct PackageSpot {
    left: f32,
    top: f32,
    lanes: Vec<LaneSpot>,
}

struct LaneSpot {
    node_x: f32,
    critic_x: Option<f32>,
}

struct Arc {
    from: f32,
    to: f32,
    approved_round: Option<u64>,
}

impl Layout {
    fn compute(map: &Flow) -> Self {
        let ticket_x = 8.;
        let ticket_label_x = ticket_x + TICKET_W + 12.;
        let mut cursor = ticket_label_x + TICKET_LABEL_W + 30.;

        let mut design_x = Vec::new();
        let mut node_x: HashMap<String, f32> = HashMap::new();
        for step in &map.design {
            design_x.push(cursor);
            node_x.insert(step.id.clone(), cursor);
            cursor += STEP_PITCH;
        }
        // The track's last point before the build fan: a step's pitch past the
        // final marker, or the cursor when there are no design steps at all.
        let fan_x = if design_x.is_empty() { cursor } else { cursor - STEP_PITCH + 66. };

        let package_left = fan_x + 44.;
        let mut packages = Vec::new();
        let mut package_top = TRACK_Y + 52.;
        for package in &map.packages {
            let mut lanes = Vec::new();
            for lane in &package.lanes {
                let lane_x = package_left + 96.;
                let critic_x = lane.critic.as_ref().map(|_| package_left + 168.);
                node_x.insert(lane.node.id.clone(), lane_x);
                if let (Some(critic), Some(critic_x)) = (&lane.critic, critic_x) {
                    node_x.insert(critic.id.clone(), critic_x);
                }
                lanes.push(LaneSpot { node_x: lane_x, critic_x });
            }
            let height = PKG_HEAD + LANE_H * package.lanes.len() as f32;
            packages.push(PackageSpot { left: package_left, top: package_top, lanes });
            package_top += height + PKG_GAP;
        }
        let package_bottom =
            if packages.is_empty() { TRACK_Y + 52. } else { package_top - PKG_GAP };
        let fan_in_x = package_left + PKG_W + 44.;
        let integrate_x = fan_in_x + 28.;

        // A design loop spans its two markers. A build loop returns to an
        // Implementer lane, and the node that sent it back is usually a
        // reviewer with no marker of its own — so it arches over the lane.
        let mut arcs = Vec::new();
        for looped in &map.design_loops {
            if let (Some(&from), Some(&to)) = (node_x.get(&looped.from), node_x.get(&looped.to)) {
                arcs.push(Arc { from, to, approved_round: looped.approved_round });
            }
        }
        // Several loops can return to one Implementer (typecheck, test and
        // reviewer all retry it), and every lane shares its x — so group the
        // build loops by target and draw one arc per target. Otherwise the
        // "revise" label and the round pill stack on the same spot.
        let mut build_arcs: Vec<Arc> = Vec::new();
        for looped in &map.build_loops {
            let Some(&to) = node_x.get(&looped.to) else { continue };
            // x is a whole-pixel coordinate, so half a pixel identifies the span.
            match build_arcs.iter_mut().find(|arc| (arc.to - (to + 88.)).abs() < 0.5) {
                Some(arc) => {
                    // A round is only shown when every merged loop agrees there
                    // is one; keep the highest when they do.
                    arc.approved_round = match (arc.approved_round, looped.approved_round) {
                        (Some(earliest), Some(round)) => Some(earliest.max(round)),
                        _ => None,
                    };
                }
                None => build_arcs.push(Arc {
                    from: to - 88.,
                    to: to + 88.,
                    approved_round: looped.approved_round,
                }),
            }
        }
        arcs.extend(build_arcs);

        let tiers_bottom = if map.integrate.is_some() {
            TRACK_Y + 26. + 18. * map.tiers.len() as f32
        } else {
            TRACK_Y + 20.
        };
        let height = package_bottom.max(tiers_bottom).max(TRACK_Y + 40.) + 24.;
        let width = (integrate_x + PILL_W).max(integrate_x + TIERS_W) + 24.;

        Self {
            width,
            height,
            ticket_x,
            ticket_label_x,
            design_x,
            packages,
            fan_x,
            fan_in_x,
            integrate_x,
            arcs,
        }
    }
}

/// One stroked connector, in the map's own coordinates.
struct Stroke {
    from: (f32, f32),
    to: (f32, f32),
    ctrl: Option<((f32, f32), (f32, f32))>,
    colour: Hsla,
    width: f32,
}

impl Stroke {
    fn line(from: (f32, f32), to: (f32, f32), colour: Hsla, width: f32) -> Self {
        Self { from, to, ctrl: None, colour, width }
    }

    fn cubic(
        from: (f32, f32),
        to: (f32, f32),
        ctrl1: (f32, f32),
        ctrl2: (f32, f32),
        colour: Hsla,
        width: f32,
    ) -> Self {
        Self { from, to, ctrl: Some((ctrl1, ctrl2)), colour, width }
    }
}

/// The colour of a step's state, as the design's table spells it.
fn state_colour(state: &str) -> u32 {
    match state {
        "approved" => p::GREEN,
        "in_progress" => p::BLUE,
        "sent_back" => p::ORANGE,
        "failed" => p::RED,
        // The ticket is the one "done" step, and it draws in the text colour.
        "done" => p::TEXT,
        _ => p::BORDER2,
    }
}

fn edge_width(state: &str) -> f32 {
    if state == "pending" || state == "skipped" { EDGE_W_PENDING } else { EDGE_W }
}

/// Worst-first, the same ranking `run_flow._RANK` uses.
fn rank(state: &str) -> u8 {
    match state {
        "sent_back" | "failed" => 0,
        "in_progress" => 1,
        "approved" | "done" => 3,
        _ => 2,
    }
}

fn worst<'a>(states: impl Iterator<Item = &'a str>) -> String {
    states
        .min_by_key(|state| rank(state))
        .unwrap_or("pending")
        .to_string()
}

pub(crate) fn render_flow_part_a(
    page: &MiniOrkPage,
    flow: &Value,
    scroll: &ScrollHandle,
    window: &mut Window,
    cx: &mut Context<MiniOrkPage>,
) -> AnyElement {
    let map = Flow::parse(flow);
    let layout = Layout::compute(&map);
    let strokes = connectors(&map, &layout);
    let mono = p::mono_font(cx);
    let mut pieces: Vec<AnyElement> = Vec::new();

    // The connectors go in first: GPUI paints children in insertion order, so
    // every marker and label below lands on top of the strokes.
    pieces.push(paint_strokes(strokes));

    // Ticket in: a thick bar, with the run's id under its title.
    pieces.push(
        div()
            .absolute()
            .left(px(layout.ticket_x))
            .top(px(TRACK_Y - TICKET_H / 2.))
            .w(px(TICKET_W))
            .h(px(TICKET_H))
            .rounded(px(3.))
            .bg(c(p::TEXT))
            .into_any_element(),
    );
    pieces.push(
        div()
            .absolute()
            .left(px(layout.ticket_label_x))
            .top(px(TRACK_Y - 20.))
            .w(px(TICKET_LABEL_W))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(c(p::TEXT))
                    .child("Ticket in"),
            )
            .child(p::text(map.ticket_id.clone(), 11., p::SUB).truncate())
            .into_any_element(),
    );

    // Design: a marker per step, its score above, its label and sub below.
    for (index, step) in map.design.iter().enumerate() {
        let centre_x = layout.design_x[index];
        let colour = state_colour(&step.state);
        if !step.score.is_empty() {
            pieces.push(
                div()
                    .absolute()
                    .left(px(centre_x - 60.))
                    .top(px(TRACK_Y - 34.))
                    .w(px(120.))
                    .text_center()
                    .text_size(px(12.))
                    .font(mono.clone())
                    .text_color(c(p::GREEN))
                    .child(step.score.clone())
                    .into_any_element(),
            );
        }
        let mut hit = div()
            .id(("flow-step", index))
            .absolute()
            .left(px(centre_x - 15.))
            .top(px(TRACK_Y - 15.))
            .size(px(30.))
            .flex()
            .items_center()
            .justify_center();
        if let Some(handler) = page.action_handler(Some(&step.action), cx) {
            hit = hit.cursor_pointer().on_click(handler);
        }
        pieces.push(hit.child(marker_ring(colour, step.state == "skipped")).into_any_element());
        // The state table draws a skipped step's label in SUB, the rest in TEXT.
        let label_colour = if step.state == "skipped" { p::SUB } else { p::TEXT };
        pieces.push(
            div()
                .absolute()
                .left(px(centre_x - 62.))
                .top(px(TRACK_Y + 16.))
                .w(px(124.))
                .text_center()
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(c(label_colour))
                .child(step.label.clone())
                .into_any_element(),
        );
        if !step.sub.is_empty() {
            pieces.push(
                div()
                    .absolute()
                    .left(px(centre_x - 62.))
                    .top(px(TRACK_Y + 33.))
                    .w(px(124.))
                    .text_center()
                    .text_size(px(11.))
                    .text_color(c(p::SUB))
                    .child(step.sub.clone())
                    .into_any_element(),
            );
        }
    }

    // Build: one box per package, one lane line per lane with its markers.
    for (package_index, package) in map.packages.iter().enumerate() {
        let spot = &layout.packages[package_index];
        let height = PKG_HEAD + LANE_H * package.lanes.len() as f32;
        pieces.push(
            div()
                .absolute()
                .left(px(spot.left))
                .top(px(spot.top))
                .w(px(PKG_W))
                .h(px(height))
                .rounded(px(12.))
                .bg(ca(0x363c4766))
                .into_any_element(),
        );
        // "PACKAGE n" only earns its place once there is more than one box.
        if map.packages.len() >= 2 && !package.label.is_empty() {
            pieces.push(
                div()
                    .absolute()
                    .left(px(spot.left + PKG_W - 88.))
                    .top(px(spot.top + 6.))
                    .w(px(80.))
                    .text_right()
                    .text_size(px(10.))
                    .font(mono.clone())
                    .text_color(c(p::SUB))
                    .child(package.label.clone())
                    .into_any_element(),
            );
        }
        for (lane_index, lane) in package.lanes.iter().enumerate() {
            let lane_spot = &spot.lanes[lane_index];
            let lane_y = spot.top + PKG_HEAD + LANE_H * lane_index as f32 + LANE_H / 2.;
            let colour = state_colour(&lane.node.state);
            pieces.push(
                div()
                    .absolute()
                    .left(px(spot.left + 16.))
                    .top(px(lane_y - 20.))
                    .text_size(px(10.5))
                    .text_color(c(p::SUB))
                    .child(lane.role.clone())
                    .into_any_element(),
            );
            pieces.push(
                div()
                    .absolute()
                    .left(px(spot.left + 14.))
                    .top(px(lane_y - 1.))
                    .w(px(PKG_W - 28.))
                    .h(px(2.))
                    .rounded(px(1.))
                    .bg(c(colour))
                    .into_any_element(),
            );
            pieces.push(lane_marker(page, &lane.node, lane_spot.node_x, lane_y, cx));
            if let (Some(critic), Some(critic_x)) = (&lane.critic, lane_spot.critic_x) {
                pieces.push(lane_marker(page, critic, critic_x, lane_y, cx));
            }
        }
    }

    // The revise loops: an orange arc, "revise" at its peak, and the round the
    // source finally passed in when it did.
    for (index, arc) in layout.arcs.iter().enumerate() {
        let apex_x = (arc.from + arc.to) / 2.;
        pieces.push(
            div()
                .absolute()
                .left(px(apex_x - 60.))
                .top(px(TRACK_Y - ARC_APEX_DY - 14.))
                .w(px(120.))
                .text_center()
                .text_size(px(11.))
                .text_color(c(p::SUB))
                .child("revise")
                .into_any_element(),
        );
        if let Some(round) = arc.approved_round {
            pieces.push(
                div()
                    .absolute()
                    .left(px(apex_x - 62.))
                    .top(px(TRACK_Y - ARC_APEX_DY - 36.))
                    .w(px(124.))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .id(("flow-approved", index))
                            .px(px(8.))
                            .py(px(2.))
                            .rounded(px(10.))
                            .border_1()
                            .border_color(c(p::ORANGE))
                            .text_size(px(10.5))
                            .text_color(c(p::ORANGE))
                            .child(format!("approved in round {round}")),
                    )
                    .into_any_element(),
            );
        }
    }

    // Integrate: the pill on the track, its captions stacked above it so
    // neither reaches the pill's top edge (TRACK_Y - PILL_H / 2), and its
    // tiers below.
    if let Some(step) = &map.integrate {
        let colour = state_colour(&step.state);
        pieces.push(
            div()
                .absolute()
                .left(px(layout.integrate_x - 20.))
                .top(px(TRACK_Y - PILL_H / 2. - 40.))
                .w(px(PILL_W + 40.))
                .line_height(px(18.))
                .text_center()
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(c(p::TEXT))
                .child(step.label.clone())
                .into_any_element(),
        );
        pieces.push(
            div()
                .absolute()
                .left(px(layout.integrate_x - 20.))
                .top(px(TRACK_Y - PILL_H / 2. - 20.))
                .w(px(PILL_W + 40.))
                .line_height(px(14.))
                .text_center()
                .text_size(px(11.))
                .text_color(c(p::SUB))
                .child(step.sub.clone())
                .into_any_element(),
        );
        pieces.push(
            div()
                .absolute()
                .left(px(layout.integrate_x))
                .top(px(TRACK_Y - PILL_H / 2.))
                .w(px(PILL_W))
                .h(px(PILL_H))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_3()
                .border_color(c(p::TEXT))
                .child(pill_glyph(&step.state, colour))
                .into_any_element(),
        );
    }
    for (index, tier) in map.tiers.iter().enumerate() {
        let failed = tier.state == "failed";
        let colour = if failed { p::RED } else { p::GREEN };
        let filled = if tier.total > 0. { (tier.passed / tier.total).clamp(0., 1.) } else { 0. };
        pieces.push(
            h_flex()
                .absolute()
                .left(px(layout.integrate_x))
                .top(px(TRACK_Y + 26. + 18. * index as f32))
                .gap(px(6.))
                .items_center()
                .child(
                    div()
                        .w(px(84.))
                        .text_size(px(12.))
                        .text_color(c(p::BODY))
                        .truncate()
                        .child(tier.label.clone()),
                )
                .child(
                    div().w(px(80.)).h(px(4.)).rounded(px(2.)).bg(c(p::BORDER2)).child(
                        div()
                            .w(px(80. * filled))
                            .h(px(4.))
                            .rounded(px(2.))
                            .bg(c(colour)),
                    ),
                )
                .into_any_element(),
        );
    }

    let surface = div()
        .relative()
        .w(px(layout.width))
        .h(px(layout.height))
        .children(pieces);
    let scroller = div()
        .id("flow-scroll")
        .w_full()
        .overflow_x_scroll()
        .track_scroll(scroll)
        .child(surface);
    v_flex()
        .w_full()
        .min_w_0()
        .p(px(14.))
        .rounded(px(8.))
        .border_1()
        .border_color(c(p::BORDER))
        .bg(c(p::CHROME))
        .child(
            div().relative().w_full().child(scroller).custom_scrollbars(
                Scrollbars::always_visible(ScrollAxes::Horizontal).tracked_scroll_handle(scroll),
                window,
                cx,
            ),
        )
        .into_any_element()
}

/// A marker inside a build lane: the same ring the design markers use, at the
/// lane's own anchor.
fn lane_marker(
    page: &MiniOrkPage,
    step: &Step,
    centre_x: f32,
    centre_y: f32,
    cx: &mut Context<MiniOrkPage>,
) -> AnyElement {
    let colour = state_colour(&step.state);
    let mut hit = div()
        .id(SharedString::from(format!("flow-lane-{}", step.id)))
        .absolute()
        .left(px(centre_x - 15.))
        .top(px(centre_y - 15.))
        .size(px(30.))
        .flex()
        .items_center()
        .justify_center();
    if let Some(handler) = page.action_handler(Some(&step.action), cx) {
        hit = hit.cursor_pointer().on_click(handler);
    }
    hit.child(marker_ring(colour, step.state == "skipped")).into_any_element()
}

/// The ring and its inner dot, in the step's state colour. A skipped step is
/// drawn hollow — the ring alone.
fn marker_ring(colour: u32, hollow: bool) -> Div {
    let ring = div()
        .size(px(MARKER_D))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(c(p::CARD))
        .border_3()
        .border_color(c(colour));
    if hollow {
        ring
    } else {
        ring.child(div().size(px(DOT_D)).rounded_full().bg(c(colour)))
    }
}

/// What the integrate pill shows: three dots while the tiers run, then the
/// verdict.
fn pill_glyph(state: &str, colour: u32) -> AnyElement {
    match state {
        "approved" | "done" => p::text("✓", 16., p::GREEN).into_any_element(),
        "failed" => p::text("✗", 16., p::RED).into_any_element(),
        "in_progress" => h_flex()
            .gap(px(3.))
            .children((0..3).map(|_| {
                div().size(px(4.)).rounded_full().bg(c(colour)).into_any_element()
            }))
            .into_any_element(),
        _ => div().into_any_element(),
    }
}

/// Every connector, in one canvas. The chart is small and fully known at
/// paint time, so there is no hit-testing or model state to keep.
fn paint_strokes(strokes: Vec<Stroke>) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            let origin = bounds.origin;
            let at = |x: f32, y: f32| -> Point<Pixels> { point(origin.x + px(x), origin.y + px(y)) };
            for stroke in &strokes {
                let mut path = PathBuilder::stroke(px(stroke.width));
                path.move_to(at(stroke.from.0, stroke.from.1));
                match stroke.ctrl {
                    Some((ctrl1, ctrl2)) => path.cubic_bezier_to(
                        at(stroke.to.0, stroke.to.1),
                        at(ctrl1.0, ctrl1.1),
                        at(ctrl2.0, ctrl2.1),
                    ),
                    None => path.line_to(at(stroke.to.0, stroke.to.1)),
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, stroke.colour);
                }
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
    .into_any_element()
}

/// The track, the build fan, and the revise arcs — each coloured by the state
/// of the step it leads to.
fn connectors(map: &Flow, layout: &Layout) -> Vec<Stroke> {
    let mut strokes = Vec::new();
    let integrate_state =
        map.integrate.as_ref().map(|step| step.state.as_str()).unwrap_or("pending");
    let build_state = worst(
        map.packages.iter().flat_map(|package| package.lanes.iter()).map(|lane| lane.node.state.as_str()),
    );
    let build_state = if map.packages.is_empty() { integrate_state } else { build_state.as_str() };

    // Ticket → the first design marker (or straight to the build fan).
    let first_x = layout.design_x.first().copied().unwrap_or(layout.fan_x);
    let first_state =
        map.design.first().map(|step| step.state.as_str()).unwrap_or(build_state);
    strokes.push(Stroke::line(
        (layout.ticket_x + TICKET_W, TRACK_Y),
        (first_x, TRACK_Y),
        c(state_colour(first_state)),
        edge_width(first_state),
    ));

    // Marker → marker, then the last marker → the fan point.
    for index in 1..layout.design_x.len() {
        let state = map.design[index].state.as_str();
        strokes.push(Stroke::line(
            (layout.design_x[index - 1], TRACK_Y),
            (layout.design_x[index], TRACK_Y),
            c(state_colour(state)),
            edge_width(state),
        ));
    }
    if let Some(&last) = layout.design_x.last() {
        strokes.push(Stroke::line(
            (last, TRACK_Y),
            (layout.fan_x, TRACK_Y),
            c(state_colour(build_state)),
            edge_width(build_state),
        ));
    }

    // Build: the track fans out into every lane, and every lane back in to
    // the integrate pill.
    for (package_index, package) in map.packages.iter().enumerate() {
        let spot = &layout.packages[package_index];
        for (lane_index, lane) in package.lanes.iter().enumerate() {
            let lane_y = spot.top + PKG_HEAD + LANE_H * lane_index as f32 + LANE_H / 2.;
            let state = lane.node.state.as_str();
            strokes.push(Stroke::cubic(
                (layout.fan_x, TRACK_Y),
                (spot.left, lane_y),
                (layout.fan_x + 36., TRACK_Y),
                (spot.left - 36., lane_y),
                c(state_colour(state)),
                edge_width(state),
            ));
            strokes.push(Stroke::cubic(
                (spot.left + PKG_W, lane_y),
                (layout.fan_in_x, TRACK_Y),
                (spot.left + PKG_W + 36., lane_y),
                (layout.fan_in_x - 36., TRACK_Y),
                c(state_colour(integrate_state)),
                edge_width(integrate_state),
            ));
        }
    }
    strokes.push(Stroke::line(
        (layout.fan_in_x, TRACK_Y),
        (layout.integrate_x, TRACK_Y),
        c(state_colour(integrate_state)),
        edge_width(integrate_state),
    ));

    // Revise loops: one arc per fired loop, over the run above the track.
    for arc in &layout.arcs {
        strokes.push(Stroke::cubic(
            (arc.from, TRACK_Y - ARC_RISE),
            (arc.to, TRACK_Y - ARC_RISE),
            (arc.from, TRACK_Y - ARC_CTRL_RISE),
            (arc.to, TRACK_Y - ARC_CTRL_RISE),
            c(p::ORANGE),
            EDGE_W,
        ));
    }

    strokes
}
